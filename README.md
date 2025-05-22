# TypePy Compiler

A TypePy ([ChocoPy](chocopy.org)) compiler in Rust.

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