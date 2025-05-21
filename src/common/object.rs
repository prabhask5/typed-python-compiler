use std::ptr::*;

pub const POINTER_SIZE: u32 = 8;
pub const FUNCTION_POINTER_SIZE: u32 = 8;

#[repr(i32)] // Each enum value is i32.
pub enum Type {
    Other = 0,
    Int = 1,
    Bool = 2,
    Str = 3,
    ValueList = -1, // Represents list of primitives (i.e ints, bools).
    ObjList = -2, // Represents list of object references (i.e strings, custom objects).
}

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
pub struct Prototype { // Defines the headers for an object prototype. An actual object memory layout is Prototype + object method function pointers.
    pub size: i32, // Represents size (>= 0) for normal object, < 0 for arrays object.
    pub type_tag: Type,
    pub map: *const u8, // Points to the dynamic dispatch method table for the object.
    // ... Object method pointers (right after header in memory).
}
pub const PROTOTYPE_SIZE_OFFSET: u32 = 0;
pub const PROTOTYPE_TAG_OFFSET: u32 = 4;
pub const PROTOTYPE_MAP_OFFSET: u32 = 8;
pub const PROTOTYPE_INIT_OFFSET: u32 = 16;
pub const OBJECT_PROTOTYPE_SIZE: u32 = 24;
pub const NUM_PROTOTYPE_HEADERS: u32 = 3;

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
#[allow(dead_code)] // Used in GC.
pub struct Object {
    pub prototype: *const Prototype,
    pub gc_count: u64,
    pub gc_next: Option<NonNull<Object>>, // In mark and sweep, represents TODO
    // followed by attributes
}

pub const OBJECT_PROTOTYPE_OFFSET: u32 = 0;
pub const OBJECT_GC_COUNT_OFFSET: u32 = OBJECT_PROTOTYPE_OFFSET + 8;
pub const OBJECT_GC_NEXT_OFFSET: u32 = OBJECT_GC_COUNT_OFFSET + 8;
pub const OBJECT_ATTRIBUTE_OFFSET: u32 = OBJECT_GC_NEXT_OFFSET + 8;
pub const OBJECT_HEADER_MEMBER_COUNT: u32 = 3;

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
#[allow(dead_code)] // Used in GC.
pub struct ArrayObject {
    pub object: Object,
    pub len: u64,
}

pub const ARRAY_LEN_OFFSET: u32 = OBJECT_ATTRIBUTE_OFFSET;
pub const ARRAY_ELEMENT_OFFSET: u32 = ARRAY_LEN_OFFSET + 8;

#[repr(C)] // Makes sure the struct is not reordered by the Rust compiler.
pub struct InitParam {
    pub bottom_frame: *const u64,
    pub global_section: *const u64,
    pub global_size: u64,
    pub global_map: *const u8,
    pub str_prototype: *const Prototype,
}

pub const BOTTOM_FRAME_OFFSET: u32 = 0;
pub const GLOBAL_SECTION_OFFSET: u32 = BOTTOM_FRAME_OFFSET + POINTER_SIZE;
pub const GLOBAL_SIZE_OFFSET: u32 = GLOBAL_SECTION_OFFSET + POINTER_SIZE;
pub const GLOBAL_MAP_OFFSET: u32 = GLOBAL_SIZE_OFFSET + 8;
pub const STR_PROTOTYPE_OFFSET: u32 = GLOBAL_MAP_OFFSET + POINTER_SIZE;
pub const INIT_PARAM_SIZE: u32 = std::mem::size_of::<InitParam>() as u32;
