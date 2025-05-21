mod object;

use object::*;
use std::cell::*;
use std::mem::*;
use std::process::{abort, exit};
use std::ptr::*;

mod gc;

/// Allocation unit used to measure memory usage in the mark-and-sweep GC.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct AllocUnit(u64);

// Thread-local global variables to hold runtime and GC metadata.
thread_local! {
    // Points to initialization parameters passed to runtime.
    static INIT_PARAM: Cell<*const InitParam> = const { Cell::new(std::ptr::null()) };
    // Head of the linked list of all allocated GC-tracked objects.
    static GC_HEAD: Cell<Option<NonNull<Object>>> = const { Cell::new(None) };
    // Total space currently allocated (in AllocUnits).
    static CURRENT_SPACE: Cell<usize> = const { Cell::new(0) };
    // Threshold at which the GC should trigger a collection.
    static THRESHOLD_SPACE: Cell<usize> = const { Cell::new(1024) };
}

/// Helper to round up memory allocation to nearest unit.
fn divide_up(value: usize) -> usize {
    let align = size_of::<AllocUnit>();
    if value == 0 {
        0
    } else {
        1 + (value - 1) / align
    }
}

/// Computes the size of an object in allocation units.
/// Handles both fixed-size and array-based objects.
///
/// # Safety
/// - `prototype` must be non-null and valid.
/// - For arrays, `len` must return valid length.
pub(crate) unsafe fn calculate_size<F: FnOnce() -> u64>(
    prototype: *const Prototype,
    len: F,
) -> usize {
    unsafe {
        let size = (*prototype).size;
        divide_up(if size >= 0 {
            size_of::<Object>() + size as usize
        } else {
            size_of::<ArrayObject>() + (-size as u64 * len()) as usize
        })
    }
}

/// Allocates a new TypePy object and tracks it for garbage collection.
/// Triggers GC if allocation exceeds current threshold.
///
/// # Safety
/// - Called only after runtime is initialized.
/// - `prototype` must be valid.
/// - If allocating an array, `len` must be meaningful.
#[unsafe(export_name = "$alloc_obj")]
pub unsafe extern "C" fn alloc_obj(
    prototype: *const Prototype,
    len: u64,
    rbp: *const u64,
    rsp: *const u64,
) -> *mut Object {
    unsafe {
        // Check if we need to run GC before allocating
        if CURRENT_SPACE.with(|current_space| current_space.get())
            >= THRESHOLD_SPACE.with(|threshold_space| threshold_space.get())
        {
            gc::perform_mark_and_sweep_gc(rbp, rsp);
            let current = CURRENT_SPACE.with(|current_space| current_space.get());
            let threshold = std::cmp::max(1024, current * 2);
            THRESHOLD_SPACE.with(|threshold_space| threshold_space.set(threshold));
        }

        // Calculate size in allocation units
        let size = calculate_size(prototype, || len);

        // Allocate raw memory for the object
        let pointer = Box::into_raw(vec![AllocUnit(0); size].into_boxed_slice())
            as *mut AllocUnit as *mut Object;

        // Update GC memory tracking
        CURRENT_SPACE.with(|current_space| current_space.set(current_space.get() + size));

        // Insert new object at the head of the GC list
        let gc_next = GC_HEAD.with(|gc_next| gc_next.replace(NonNull::new(pointer)));

        // Initialize object metadata
        let object = Object {
            prototype,
            gc_is_marked: 0,
            gc_next,
        };

        // If object is not an array, write Object struct directly
        if (*prototype).size >= 0 {
            pointer.write(object);
        } else {
            // For arrays, wrap in ArrayObject
            let object = ArrayObject { object, len };
            (pointer as *mut ArrayObject).write(object);
        }

        pointer
    }
}

/// Returns the length of an array-like object.
///
/// # Safety
/// - `pointer` must be a valid object allocated by `alloc_obj`.
#[unsafe(export_name = "$len")]
pub unsafe extern "C" fn len(pointer: *mut Object) -> i32 {
    unsafe {
        if pointer.is_null() {
            invalid_arg();
        }
        let object = pointer as *mut ArrayObject;
        let prototype = (*object).object.prototype;
        if !matches!(
            (*prototype).type_tag,
            Type::Str | Type::ValueList | Type::ObjList
        ) {
            invalid_arg();
        }
        (*object).len as i32
    }
}

/// Prints a TypePy object to standard output.
/// Supports int, bool, and str types.
///
/// # Safety
/// - `pointer` must be valid and initialized.
#[unsafe(export_name = "$print")]
pub unsafe extern "C" fn print(pointer: *mut Object) -> *mut u8 {
    unsafe {
        if pointer.is_null() {
            invalid_arg();
        }
        let prototype = (*pointer).prototype;
        match (*prototype).type_tag {
            Type::Int => {
                println!("{}", *(pointer.offset(1) as *const i32));
            }
            Type::Bool => {
                println!(
                    "{}",
                    if *(pointer.offset(1) as *const bool) {
                        "True"
                    } else {
                        "False"
                    }
                );
            }
            Type::Str => {
                let object = pointer as *mut ArrayObject;
                let slice = std::str::from_utf8(std::slice::from_raw_parts(
                    object.offset(1) as *const u8,
                    (*object).len as usize,
                ))
                .unwrap_or_else(|e| fatal(&e.to_string()));
                println!("{}", slice);
            }
            _ => {
                invalid_arg();
            }
        }

        std::ptr::null_mut()
    }
}

/// Reads a line from stdin into a new str object.
///
/// # Safety
/// - `init` must be called.
/// - `rbp` and `rsp` must describe a valid stack frame.
#[unsafe(export_name = "$input")]
pub unsafe extern "C" fn input(rbp: *const u64, rsp: *const u64) -> *mut Object {
    unsafe {
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .unwrap_or_else(|e| fatal(&e.to_string()));
        let mut input = input.as_bytes();
        while let Some((b'\n' | b'\r', rest)) = input.split_last() {
            input = rest;
        }

        let str_proto = INIT_PARAM.with(|init_param| (*init_param.get()).str_prototype);
        let pointer = alloc_obj(str_proto, input.len() as u64, rbp, rsp);
        std::ptr::copy_nonoverlapping(
            input.as_ptr(),
            (pointer as *mut u8).add(size_of::<ArrayObject>()),
            input.len(),
        );
        pointer
    }
}

/// Sets up runtime with initial parameters.
///
/// # Safety
/// - Must be called before any allocations or runtime calls.
#[unsafe(export_name = "$init")]
pub unsafe extern "C" fn init(init_param: *const InitParam) {
    INIT_PARAM.with(|i| i.set(init_param));
}

/// Aborts the program with a fatal error message.
pub(crate) fn fatal(message: &str) -> ! {
    eprintln!("Fatal error: {}", message);
    abort();
}

/// Terminates the program with a given exit code.
fn exit_code(code: i32) -> ! {
    println!("Exited with error code {}", code);
    exit(code);
}

/// Signals a runtime type or argument error.
fn invalid_arg() -> ! {
    println!("Invalid argument");
    exit_code(1)
}

/// Runtime trap: division by zero.
#[unsafe(export_name = "$div_zero")]
pub extern "C" fn div_zero() -> ! {
    println!("Division by zero");
    exit_code(2)
}

/// Runtime trap: index out of bounds.
#[unsafe(export_name = "$out_of_bound")]
pub extern "C" fn out_of_bound() -> ! {
    println!("Index out of bounds");
    exit_code(3)
}

/// Runtime trap: operation on None.
#[unsafe(export_name = "$none_op")]
pub extern "C" fn none_op() -> ! {
    println!("Operation on None");
    exit_code(4)
}

#[cfg(not(test))]
pub mod crt0_glue {
    unsafe extern "C" {
        #[link_name = "$typepy_main"]
        fn typepy_main();
    }

    /// Entry point that invokes the compiled TypePy program.
    ///
    /// # Safety
    /// - Assumes a valid `$typepy_main` symbol exists.
    #[unsafe(export_name = "main")]
    pub unsafe extern "C" fn entry_point() -> i32 {
        unsafe { typepy_main(); }
        0
    }
}
