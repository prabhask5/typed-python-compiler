mod object;

use object::*;
use std::cell::*;
use std::mem::*;
use std::process::{abort, exit};
use std::ptr::*;

mod gc;

#[repr(transparent)]
#[derive(Clone, Copy)]
struct AllocUnit(u64);

thread_local! {
    static INIT_PARAM: Cell<*const InitParam> = const { Cell::new(std::ptr::null()) };
    static GC_HEAD: Cell<Option<NonNull<Object>>> = const { Cell::new(None) };
    static CURRENT_SPACE: Cell<usize> = const { Cell::new(0) };
    static THRESHOLD_SPACE: Cell<usize> = const { Cell::new(1024) };
}

fn divide_up(value: usize) -> usize {
    let align = size_of::<AllocUnit>();
    if value == 0 {
        0
    } else {
        1 + (value - 1) / align
    }
}

/// # Safety
///  - `*prototype` is not null.
///  - Safety requirement for `Prototype`.
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

/// Allocates a TypePy object
///
/// # Safety
///  - `init` already called.
///  - `prototype` is not null.
///  - `*prototype` content never changes after calling this function.
///  - Other safety requirement for `Prototype`.
///  - `rbp` and `rsp` points to the bottom and the top of the top stack frame.
///  - For the returned object, any fields in Object (header) must never be changed.
///  - If the `prototype` indicates an array object,
///    for the returned object, any fields in ArrayObject (header) must never be changed.
///  - All attributes in the object must maintain valid values according to the type tag and reference map.
#[unsafe(export_name = "$alloc_obj")]
pub unsafe extern "C" fn alloc_obj(
    prototype: *const Prototype,
    len: u64,
    rbp: *const u64,
    rsp: *const u64,
) -> *mut Object {
    unsafe {
        if CURRENT_SPACE.with(|current_space| current_space.get())
            >= THRESHOLD_SPACE.with(|threshold_space| threshold_space.get())
        {
            gc::collect(rbp, rsp);
            let current = CURRENT_SPACE.with(|current_space| current_space.get());
            let threshold = std::cmp::max(1024, current * 2);
            THRESHOLD_SPACE.with(|threshold_space| threshold_space.set(threshold));
        }

        let size = calculate_size(prototype, || len);

        let pointer =
            Box::into_raw(vec![AllocUnit(0); size].into_boxed_slice()) as *mut AllocUnit as *mut Object;

        CURRENT_SPACE.with(|current_space| current_space.set(current_space.get() + size));

        let gc_next = GC_HEAD.with(|gc_next| gc_next.replace(NonNull::new(pointer)));

        let object = Object {
            prototype,
            gc_is_marked: 0,
            gc_next,
        };

        if (*prototype).size >= 0 {
            pointer.write(object);
        } else {
            let object = ArrayObject { object, len };
            (pointer as *mut ArrayObject).write(object);
        }

        pointer
    }
}

/// Gets the array length of a TypePy object
///
/// # Safety
///  - `init` is already called.
///  - `pointer` must be previously returned by `alloc_obj`.
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

/// Prints a TypePy object
///
/// # Safety
///  - `init` is already called.
///  - `pointer` must be previously returned by `alloc_obj`.
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

/// Creates a new str object that holds a line of user input
///
/// # Safety
///  - `init` is already called.
///  - `rbp` and `rsp` points to the bottom and the top of the top stack frame.
///  - For the returned object, any fields in ArrayObject (header) must never be changed.
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

/// Initialize runtime
///
/// # Safety
///  - `init_param` is not null.
///  - `*init_param` content never changes after calling this function.
///  - Other safety requirements on `InitParam`.
#[unsafe(export_name = "$init")]
pub unsafe extern "C" fn init(init_param: *const InitParam) {
    INIT_PARAM.with(|i| i.set(init_param));
}

pub(crate) fn fatal(message: &str) -> ! {
    eprintln!("Fatal error: {}", message);
    abort();
}

fn exit_code(code: i32) -> ! {
    println!("Exited with error code {}", code);
    exit(code);
}

fn invalid_arg() -> ! {
    println!("Invalid argument");
    exit_code(1)
}

#[unsafe(export_name = "$div_zero")]
pub extern "C" fn div_zero() -> ! {
    println!("Division by zero");
    exit_code(2)
}

#[unsafe(export_name = "$out_of_bound")]
pub extern "C" fn out_of_bound() -> ! {
    println!("Index out of bounds");
    exit_code(3)
}

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

    /// # Safety
    /// `$typepy_main` is linked to a valid TypePy program entry point
    #[unsafe(export_name = "main")]
    pub unsafe extern "C" fn entry_point() -> i32 {
        unsafe { typepy_main(); }
        0
    }
}
