# Typed Python Language Rust Compiler

> [ChocoPy](https://chocopy.org/) is a programming language designed for classroom use in undergraduate compilers courses. ChocoPy is a restricted subset of Python 3, which can easily be compiled to a target such as RISC-V. The language is fully specified using formal grammar, typing rules, and operational semantics. ChocoPy is used to teach CS 164 at UC Berkeley. ChocoPy has been designed by Rohan Padhye and Koushik Sen, with substantial contributions from Paul Hilfinger.

This is a Rust variant of the ChocoPy compiler created as part of the COMPSCI 164 class project that now targets x86.

## Get Started

To build the main compiler and standard library, run this in the base directory:

```bash
cargo build
```

Then use the compiler on the provided input files in the test/ directory, like so:

```bash
# Compile to executable output.exe, this compiler autodetects for your platform.
cargo run input.py output.exe

# Compile to object file output.o.
cargo run input.py output.o --obj

# Output untyped AST JSON to STDOUT.
cargo run input.py --ast

# Output typed AST JSON to STDOUT.
cargo run input.py --typed
```

## Compiler Features

- Features a hand written Rust lexer and parser. The parser is a left recursive parser with a look-ahead value of 2 to distinguish between declarations and statements.
- Supports outputting an intermediate AST representation of the code. This can be viewed directly through the CLI.
- Type checks the AST to predict and determine expected types for complex statements and declarations. Throws non-fatal type errors stored in the AST to see type errors in the input program. This can be viewed directly through the CLI.
- Generates x86 assembly code, and handles assembly (converting to an object file) on three different platforms: Windows, Linux, and Mac.
- Handles linking against a separate create to represent a standard library. This library handles built-in function implementation, including object allocation and error reporting. This object allocation also executes the garbage collector.
- Implements the mark-and-sweep garbage collection algorithm. This garbage collector is called whenever a new object is allocated, and the total size of allocated objects reaches a threshold.
    - To do this, the system walks through all the objects that are heap allocated (global and local references) and marks which ones are currently being used. The system accomplishes this by storing a linked list of objects that are currently on the heap, to make it easy to track every dynamically allocated object.
    - The system also recursively tracks member object references within one particular object to mark non-global objects.
    - Then in the sweep phase, we walk through the heap linked list again and free all the un-marked objects.
 
## Design Overview

### Parser

The compiler uses a hand-written lexer and parser.

- **Lexer**: Implemented as a generator-like component. Since stable Rust does not support generators, this is simulated using an asynchronous pipe model.
- **Parser**: A recursive descent parser where each `parse_xxx(...)` function maps to a grammar non-terminal. Left recursion is rewritten into loops. Expression parsing uses precedence levels (`parse_exprN(...)`) to manage operator hierarchy. The parser uses at most 2-token lookahead, primarily to differentiate declarations from statements.

### Semantic Analysis

The compiler supports intermediate typed and untyped ASTs in JSON format, compliant with the CS 164 spec. Internally:

- AST nodes are implemented with idiomatic Rust `struct`s and `enum`s, instead of a class hierarchy.
- Pattern matching replaces virtual dispatch for semantic analysis.

### Code Generation

Unlike the [ChocoPy Implementation Guide](https://chocopy.org/chocopy_implementation_guide.pdf), this compiler targets x86 instead of RISC-V and diverges in several implementation details.

#### Symbol Naming

Symbol names follow these conventions:

- `$chocopy_main`: User program entry point  
- `$global`: Global variable section  
- Constructors: Use the class name (`MyClass`)  
- Methods: `<ClassName>.<MethodName>`  
- Prototypes: `<ClassName>.$proto`  
- Nested functions: `<ParentSymbol>.<FuncName>`  
- Standard library: All functions prefixed with `$` (except `main`)  

User-defined functions are not prefixed. Variable and attribute names are kept as-is. Hidden/internal attributes are prefixed with `$`.

#### Register Usage

- `RSP` and `RBP` retain their conventional roles (stack and frame pointers).
- All other general-purpose registers are used freely.

#### Object Representation

Objects are 64-bit pointers. `0` denotes `None`.

##### Unboxed Values

- `int` → 4 bytes  
- `bool` → 1 byte  

They are stored in 8-byte stack slots. In global variables and object fields, alignment is based on their actual size (packed layout).

##### Object Layout

- **Header (24 bytes)**:  
  - 8 bytes: Pointer to `$proto`  
  - 16 bytes: Reserved for GC (`$gc_is_marked`, `$gc_next`)  
- **Attributes** follow the header
- **Array-like types** (`str`, `[T]`) add:  
  - 8-byte `$len` field  
  - Packed element layout (`int`, `bool`, `str` use 4, 1, and 1 byte respectively)

##### Prototype Objects

Every type `C` (including primitives) has a global `C.$proto` symbol pointing to a shared prototype object. Prototypes contain:

- `$size`: Object size (positive) or per-element size for arrays (negative)
- `$tag`: Type tag  
  - `0` → user-defined/built-in object  
  - `-1` → `[int]` or `[bool]` (plain list)  
  - `-2` → other lists (reference elements)  
- `$map`: Reference bitmap for GC
- Method table (starting with `__init__`)

##### Constructors

Each class `C` has a constructor symbol `C`. The constructor:

1. Allocates memory  
2. Initializes fields manually (not from prototype)  
3. Invokes `__init__`

#### Functions and Methods

##### Calling Convention

- Arguments pushed in right-to-left order
- Nested functions receive static link in `R10`
- Stack aligned to 8 mod 16
- Return values in `RAX`
- Caller restores stack
- Volatile registers: `RAX`, `RCX`, `RDX`, `RSI`, `RDI`, `R8–R11`, `R10`

**Exceptions**: `$chocopy_main` and standard library functions use the system ABI (System V or Windows).

##### Stack Frame Layout (Top to Bottom)

1. Outgoing arguments (for nested calls)
2. Alignment padding (if needed)
3. Temporaries
4. Local variables
5. Static link (`R10`)
6. Saved `RBP` (caller’s frame pointer)
7. Return address

- Local variables: in declaration order, highest at bottom
- Parameters: leftmost has lowest address (at top)

#### Execution Environment

The final binary is composed of:

- `program.o`: Compiled user program
- `chocopy_rs_std`: Standard runtime library
- `libc`: System C library

Example call chain:

main → chocopy_rs_std
$chocopy_main → program.o
print → program.o
$print → chocopy_rs_std
fwrite → libc


#### Standard Library

`chocopy_rs_std` provides:

- Built-in function implementations (`$print`, `$len`, etc.)
- Memory allocation (`$alloc`) and garbage collection
- Error handling
- Entry point `main` → calls `$chocopy_main`

To prevent conflicts with user-defined `main`, the real entry point is in the standard library.

#### Garbage Collection

Implements mark-and-sweep GC triggered by `$alloc` when a memory threshold is reached.

##### Allocation and Marking

- Objects dynamically allocated on the heap are linked together via `$gc_next`
- `$gc_is_marked` is set to `1` for reachable objects during mark phase
- Unmarked objects are deallocated in the sweep phase
- Live objects reset `$gc_is_marked` to `0`

##### Root Discovery

GC walks through:

- **Global references**: Map passed at `$init`
- **Local references**: Maps attached after function calls (via `PREFETCHNTA`)
- **Object members**: Maps in the class prototype (`$reference_bitmap`)

Reference maps are bitstrings indicating which fields are pointers. For locals, each map describes one stack frame. The GC walks the full stack to gather all active maps.
