macro_rules! verb {
    ( $verbosity:expr, $level:expr, $( $message:expr ),* ) => {
        if $verbosity >= $level {
            println!($($message),*);
        }
    };
}
