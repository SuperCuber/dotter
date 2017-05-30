macro_rules! verb {
    ( $verbosity:expr, $level:expr, $( $message:expr ),* ) => {
        if $verbosity >= $level {
            println!($($message),*);
        }
    };
}

macro_rules! or_err {
    ( $value:expr ) => {
        match $value {
            Ok(ans) => ans,
            Err(msg) => {
                println!("{}", msg);
                process::exit(1);
            }
        }
    }
}
