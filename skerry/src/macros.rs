#[macro_export]
macro_rules! create_fn_error {
    ($type:ident, [$($e:path),*], [$($starred:ident),*]) => {
        skerry::skerry_internals::paste! {
            skerry::skerry_internals::expand_starred_lists! {
                @step
                target: [$type],
                base: [[<$type:snake>]],
                accum: [$($e),*],
                remaining: [$([<$starred:snake _errors>]),*]
            }
        }
    };
}

#[macro_export]
macro_rules! expand_starred_lists {
    (@step target: [$type:ident], base: [$fn_name:ident], accum: [$(,)? $($acc:path),*], remaining: []) => {
        skerry::skerry_internals::create_fn_error_step!($type, $($acc),*);
    };

    (@step target: [$type:ident], base: [$fn_name:ident], accum: [$(,)? $($acc:path),*], remaining: [$next_macro:ident $(, $rest:ident)*]) => {
        $next_macro! {
            @callback
            target: [$type],
            base: [$fn_name],
            accum: [$($acc),*],
            remaining: [$($rest),*]
        }
    };
}

pub use create_fn_error;
pub use expand_starred_lists;
