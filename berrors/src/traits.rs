pub const trait ComparableError {
    const NAME: &'static str;
    const CODES: &'static [&'static str];
}
pub const trait ErrCode {
    const CODE: &'static str;
}

// #[diagnostic::(
//     message = "The error '{E}' is not handled by the function return type
// '{Self}'",     label = "this error is missing from the #[fn_error] list",
//     note = "Add '{E}' to your #[fn_error(...)] attribute to allow this
// conversion." )]V
pub const trait MissingConvert<T> {}
