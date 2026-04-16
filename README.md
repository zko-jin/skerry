## Skerry: Super Kool ERRors Yoh

Example:
```rust
use skerry::*;

// 1. Define your error boundary
#[skerry_mod]
pub mod errors {
    pub struct DatabaseErr;
    pub struct AuthErr;
    pub struct ValidationErr;
}

// 2. Generate a 'low_level' error enum automatically
#[skerry_fn]
fn check_auth() -> Result<(), AuthErr> {
    Err(CheckAuthError::AuthErr(AuthErr))
}

// 3. Use '&' to expand and bubble up sub-errors seamlessly
#[skerry_fn]
pub fn Controller() -> Result<(), (ValidationErr, &CheckAuthError)> {
    // ValidationErr is local, AuthErr is pulled in from check_auth via '&'
    check_auth()?;
    Ok(())
}
```

Known Issues:
- There's no macro for handling `impl` blocks
- Same for `trait` blocks

Skerry is a type-safe error management framework designed to kill boilerplate.
It allows you to define a global error set while returning granular, function-specific
enums that are automatically generated at compile-time.

### Core Workflow

1. Define all possible error structs in a `#[skerry_mod]`.
2. Mark functions with `#[skerry_fn]`.
3. Use the `&` operator to bubble up errors from sub-functions without manually mapping variants.

---

### The Error Module
Every project needs one module (usually `errors.rs`) that acts as the source of truth.

```rust
pub use skerry::*; // Required to be pub for easier macro expansions

#[skerry_mod]
mod errors {
    pub struct ErrA;
    pub struct ErrB;
    pub struct ErrC;
    pub struct DatabaseErr;
}
```
*Note: When using errors in any other file, import them via `crate::errors::*;` instead
of individual imports to ensure the macros can resolve the paths correctly.*

---

### Function-Specific Enums

By using `#[skerry_fn]`, you define a return type using a tuple of error structs.
Skerry transforms this into a unique enum named `{FunctionName}Error`.

```rust
#[skerry_fn]
pub fn low_level() -> Result<(), (ErrA, ErrB)> {
    // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
    Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
}
```

---

### The Ampersand (`&`) Expansion

The `&` operator is the heart of Skerry. When you put `&OtherFnError` in your return tuple:

* **Expansion**: It pulls all variants from `OtherFnError` into your current function's list.
* **Promotion**: It allows the `?` operator to work seamlessly for that function's return type.
* **Deduplication**: Variants are deduplicated automatically. If `ErrA` is added manually
  and also exists inside a `&` expansion, only one variant is generated.

```rust
#[skerry_fn]
pub fn high_level() -> Result<(), (ErrC, &LowLevelError)> {
    // 1. Sees ErrC -> Adds variant
    // 2. Sees &LowLevelError -> Inspects LowLevelError, finds (ErrA, ErrB)
    // 3. Final HighLevelError contains variants: ErrA, ErrB, ErrC

    low_level()?; // Bubbles up automatically
    Ok(())
}
```

The syntax below has the exact same effects, `&LowLevelError` is nothing more than syntatic sugar

```rust
#[skerry_fn]
pub fn high_level() -> Result<(), (ErrA, ErrB, ErrC)> {
    // ...
    # Ok(())
}
```

In the cases above the generated enum looks like this
```rust
pub enum HighLevelError {
    ErrA(ErrA),
    ErrB(ErrB),
    ErrC(ErrC),
}
```

---

### Compile-Time Safety

Skerry uses a custom trait system (`MissingConvert`) to verify error bounds at
compile-time. If you try to use `?` on a function whose errors are not represented
in your current return tuple, the compiler will refuse to build.

License: MIT
