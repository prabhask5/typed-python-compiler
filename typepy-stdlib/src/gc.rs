use super::*;

// Reads a little-endian i32 from a pointer to memory.
unsafe fn read_i32_le(ptr: *const u8) -> i32 {
    unsafe {
        let mut buffer = [0; 4];
        // Copy 4 bytes from the pointer into buffer.
        std::ptr::copy_nonoverlapping(ptr, buffer.as_mut_ptr(), 4);
        // Convert bytes to i32 assuming little-endian encoding.
        i32::from_le_bytes(buffer)
    }
}

// Gets the reference bitmap for the current stack frame's return address (RIP).
// This map indicates which stack slots contain object references.
unsafe fn get_reference_bitmap_from_rip(rip: *const u8) -> *const u8 {
    unsafe {
        // Read the 4-byte offset at RIP + 3.
        let offset = read_i32_le(rip.offset(3));
        // Return the address of the bitmap by offsetting RIP.
        rip.offset((offset + 7) as isize)
    }
}

// Recursively marks reachable heap objects starting from a given pointer.
// This is the "mark" phase of the mark-and-sweep garbage collector.
unsafe fn mark_reachable_from(var_ptr: *const u64) {
    unsafe {
        if *var_ptr == 0 {
            // Null pointer, nothing to mark.
            return;
        }

        let object_ptr = *var_ptr as *mut Object;
        if (*object_ptr).gc_is_marked == 1 {
            // Already marked, skip to avoid infinite recursion.
            return;
        }
        // Mark the object as reachable.
        (*object_ptr).gc_is_marked = 1;

        match (*(*object_ptr).prototype).type_tag {
            Type::Other => {
                // This is a regular object with fields, some of which may be references.
                let field_count = ((*(*object_ptr).prototype).size / 8) as usize;
                let ref_bitmap = (*(*object_ptr).prototype).reference_bitmap;
                for i in 0..field_count {
                    // Check if the i-th field is a reference by looking up the bitmap.
                    let flag = *ref_bitmap.add(i / 8) & (1 << (i % 8));
                    if flag != 0 {
                        // Follow the reference and mark recursively.
                        mark_reachable_from((object_ptr.add(1) as *const u64).add(i));
                    }
                }
            }
            Type::ObjList => {
                // This is an array of references (e.g. list of objects).
                let list_ptr = object_ptr as *mut ArrayObject;
                for i in 0..(*list_ptr).len {
                    // Follow each element of the array and mark recursively.
                    mark_reachable_from((list_ptr.add(1) as *const u64).add(i as usize));
                }
            }
            _ => (), // Other types do not contain references.
        }
    }
}

// Performs the full mark-and-sweep garbage collection.
// 1. Marks reachable objects from the stack and global variables.
// 2. Sweeps unreachable objects and reclaims memory.
pub unsafe fn perform_mark_and_sweep_gc(stack_frame_base: *const u64, stack_pointer: *const u64) {
    unsafe {
        let init_param = INIT_PARAM.with(|param| &*param.get());

        // Mark phase: Walk the stack frames and mark reachable objects.
        let mut return_address = *stack_pointer.offset(-1) as *const u8;
        let mut current_frame = stack_frame_base;
        loop {
            // Get reference bitmap from the function's return address.
            let ref_map = get_reference_bitmap_from_rip(return_address);
            // Read min and max indices of the map.
            let min_index = read_i32_le(ref_map);
            let max_index = read_i32_le(ref_map.offset(4));

            for index in min_index..=max_index {
                let map_index = (index - min_index) as usize;
                // Determine if the stack slot at this index is a reference.
                let flag = *ref_map.add(8 + map_index / 8) & (1 << (map_index % 8));
                if flag != 0 {
                    // Mark reachable object from this stack slot.
                    mark_reachable_from(current_frame.offset(index as isize));
                }
            }

            if current_frame == init_param.bottom_frame {
                // Reached bottom of stack, done marking stack roots.
                break;
            }
            // Unwind to previous frame (linked list of stack frames).
            return_address = *current_frame.offset(1) as *const u8;
            current_frame = *current_frame as *const u64;
        }

        // Mark from global variables.
        for index in 0..init_param.global_size / 8 {
            let idx = index as usize;
            // Determine if the global slot contains a reference.
            let flag = *init_param.global_map.add(idx / 8) & (1 << (idx % 8));
            if flag != 0 {
                // Mark reachable object from this global slot.
                mark_reachable_from(init_param.global_section.add(idx));
            }
        }

        // Sweep phase: Reclaim unmarked objects.
        let mut head = GC_HEAD.with(|gc_head| gc_head.get());
        let mut cursor = &mut head;
        let mut reclaimed_units = 0;

        while let Some(object) = *cursor {
            let object_ptr = object.as_ptr();
            if (*object_ptr).gc_is_marked == 1 {
                // Keep this object; reset mark for future GC.
                (*object_ptr).gc_is_marked = 0;
                cursor = &mut (*object_ptr).gc_next;
            } else {
                // This object is unreachable; remove from GC list.
                *cursor = (*object_ptr).gc_next;

                // Compute size of object in allocation units.
                let size_units = calculate_size((*object_ptr).prototype, || (*(object_ptr as *mut ArrayObject)).len);

                // Reclaim memory by dropping the boxed slice.
                drop(Box::from_raw(std::slice::from_raw_parts_mut(
                    object_ptr as *mut AllocUnit,
                    size_units,
                )));

                reclaimed_units += size_units;
            }
        }

        // Update GC state after sweeping.
        GC_HEAD.with(|gc_head| gc_head.set(head));
        CURRENT_SPACE.with(|current_space| current_space.set(current_space.get() - reclaimed_units));
    }
}
