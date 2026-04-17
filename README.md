## Skerry: Super Kool ERRors Yoh

Example:
```rust
use skerry::*;

// Define your error boundary, there can be only one #[sherry_mod] in your project
#[skerry_mod]
pub mod errors {
    pub struct DatabaseErr;
    pub struct AuthErr;
    pub struct ValidationErr;

    pub struct InvalidParse;

    pub struct LibErr(ErrorFromLib);
    impl From<ErrorFromLib> for LibErr {
        fn from(val: ErrorFromLib) -> Self {
            Self(val)
        }
    }
}

// Generates a CheckAuthError enum automatically
#[skerry_fn]
fn check_auth() -> Result<(), e![AuthErr]> {
    Err(CheckAuthError::AuthErr(AuthErr))
}


struct Controller;

#[skerry_impl(prefix(Controller))] // This allows #[skerry_fn] to run on impl blocks
impl Controller {
    // Use '*' to expand and bubble up sub-errors seamlessly.
    #[skerry_fn]
    pub fn run() -> Result<(), e![ValidationErr, LibErr, *CheckAuthError]> {
        // AuthErr is pulled in from check_auth via '*CheckAuthError'.
        check_auth()?;

        // Automatically bubble up library errors as long as an error
        // from `#[skerry_mod]` implements `From` for it.
        lib_fn_that_returns_error()?;

        Ok(())
    }
}
#[skerry_trait]
trait ToJson {
    #[skerry_fn]
    fn to_json(&self) -> Result<(), e![InvalidParse]>;
}

#[skerry_impl]
impl ToJson for Controller {
    // Whenever you do not want to generate a new error just don't use e![]
    // this will instead reuse an existing error
    #[skerry_fn]
    fn to_json(&self) -> Result<(), ToJsonError> {
        Ok(())
    }
}
```

### Core Workflow

- Define all possible error structs in a `#[skerry_mod]`.
- Mark functions with `#[skerry_fn]`.
- Use the `*` operator to bubble up errors from sub-functions without manually mapping variants.

---

### The Error Module
Every project needs one module (usually `errors.rs`) that acts as the source of truth.

```rust
pub use skerry::*; // Recommended to be pub for easier macro expansions

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
pub fn low_level() -> Result<(), e![ErrA, ErrB]> {
    // Generates LowLevelError { ErrA(ErrA), ErrB(ErrB) }
    Err(LowLevelError::ErrA(ErrA)) // You can also type Err(ErrA.into())
}
```

---

### The Asterisk (`*`) Expansion

When you put `*OtherFnError` in your return array it pulls all
variants from `OtherFnError` into your current function's list.

* **Deduplication**: Variants are deduplicated automatically. If `ErrA` is added manually
  and also exists inside a `*` expansion, only one variant is generated.

```rust
#[skerry_fn]
pub fn high_level() -> Result<(), e![ErrC, *LowLevelError]> {
    // 1. Sees ErrC -> Adds variant
    // 2. Sees *LowLevelError -> Inspects LowLevelError, finds (ErrA, ErrB)
    // 3. Final HighLevelError contains variants: ErrA, ErrB, ErrC

    low_level()?; // Bubbles up automatically
    Ok(())
}
```

The syntax below has the exact same effects, `*LowLevelError` is nothing more than syntatic sugar

```rust
#[skerry_fn]
pub fn high_level() -> Result<(), e![ErrA, ErrB, ErrC]> {
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
### Using Skerry inside Impl Blocks

Skerry provides the `#[skerry_impl]` attribute to handle methods within `impl` blocks.
This attribute coordinates with `#[skerry_fn]` to split the generated code
so error enums are generated outside the `impl` block.

#### Example

```rust
use skerry::*;

#
pub struct Database;

#[skerry_impl(prefix(Database))] // Optional prefix for functions inside impl block
impl Database {
    #[skerry_fn]
    pub fn connect(&self) -> Result<(), e![*RemoteCallError]> {
        remote_call()?;
        Ok(())
    }
}

fn main() {
    let db = Database;
    let result: Result<(), DatabaseConnectError> = db.connect();
    assert!(result.is_ok());
}
```
### Using Skerry inside Trait Blocks

Skerry provides the `#[skerry_trait]` attribute to handle methods within `trait` blocks.
This attribute coordinates with `#[skerry_fn]` to split the generated code
so error enums are generated outside the `trait` block.

#### Example

```rust
use skerry::*;

#

#[skerry_impl(prefix(ToJson))] // Optional prefix for functions inside trait block
trait ToJson {
    #[skerry_fn]
    pub fn parse(&self) -> Result<(), e![*ParseFailed]>;
}
```
---

### Compile-Time Safety

Skerry uses a custom trait system (`MissingConvert`) to verify error bounds at
compile-time. If you try to use `?` on a function whose errors are not represented
in your current return tuple, the compiler will refuse to build.

License: MIT
