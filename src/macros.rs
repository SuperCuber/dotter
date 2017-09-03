macro_rules! verb {
    ( $verbosity:expr, $level:expr, $( $message:expr ),* ) => {
        if $verbosity >= $level {
            use ansi_term;
            println!("{}", ansi_term::Colour::Yellow.paint(format!($($message),*)));
        }
    };
}

macro_rules! or_err {
    ( $value:expr ) => {
        match $value {
            Ok(ans) => ans,
            Err(msg) => {
                use ansi_term;
                println!("{}", ansi_term::Colour::Red.paint(msg));
                process::exit(1);
            }
        }
    }
}
