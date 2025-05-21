use std::ptr::*;

#[repr(i32)] // Each enum value is i32.
pub enum Type {
    Other = 0,
    Int = 1,
    Bool = 2,
    Str = 3,
    ValueList = -1, // Represents list of primitives (i.e ints, bools).
    ObjList = -2,   // Represents list of object references (i.e strings, custom objects).
}

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
// Defines the headers for an object prototype. An actual object memory layout is Prototype + object method function pointers.
pub struct Prototype {
    // Represents size (>= 0) for normal object, < 0 for arrays object.
    pub size: i32,

    pub type_tag: Type,

    // This is a pointer to a bitmap representing the member variables of the object,
    // if the nth position is 1, then the nth member variable is a reference to another object.
    pub reference_bitmap: *const u8,
    // ... Object method pointers (right after header in memory).
}

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
#[allow(dead_code)] // Used in GC.
pub struct Object {
    pub prototype: *const Prototype,
    pub gc_is_marked: u8, // In mark and sweep, represents if this object is marked for usage.
    pub gc_next: Option<NonNull<Object>>, // A pointer to the next allocated object in the heap, forming a singly linked list of all heap-allocated, GC-managed objects.
    // ... Object attributes (right after header in memory).
}

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
#[allow(dead_code)] // Used in GC.
pub struct ArrayObject {
    pub object: Object,
    pub len: u64, // Gets the number of elements in the array. Different from size in the prototype (< 0 for array objects).
    // ... Array elements (right after header in memory).
}

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
pub struct InitParam {
    pub bottom_frame: *const u64, // Stack base pointer, used for stack walking.
    pub global_section: *const u64, // Pointer to global/static variables.
    pub global_size: u64, // Size of global memory (in bytes).
    pub global_map: *const u8, // Bitmap of which globals are GC roots.
    pub str_prototype: *const Prototype, // Metadata for allocating string objects.
}
