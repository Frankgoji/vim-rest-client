/// process_while module
/// Handles while loop for vim-rest-client. A while block is defined thusly:
///
/// ###{ while {{.i < 10}}
/// <requests, variable assignments, folds>
/// ###} endwhile
///
/// The output shown for a while loop should be the result of the final loop.
/// An important feature of the while loop is that it should create ONE SSH
/// connection for any requests inside the loop (if an SSH connection is
/// configured), so that way if you are looping multiple requests less time is
/// wasted establishing multiple SSH connections in a row.
/// TODO: or maybe this should be a generic feature of process_input, to only create
/// the SSH connections that are needed, for each invocation of vim-rest-client
/// This has the advantage that While doesn't need special handling for SSH connections
/// and instead is ONLY in charge of repetition

// TODO: need Regex, need FoldEnv, parse_selector
use std::fs;
use regex::Regex;

use crate::{ENV_FILE, FoldEnv};
// TODO: instead of calling FoldEnv's make request, handle with the While
// TODO: process_while should take the input (take control of iterating over input
// hand back control once entire while has been defined)
// TODO: while should be capable of handling multiple fold envs...
// TODO: when entire While block has been defined, then start looping (or is that necessary?)

// TODO: need to define a While struct
// TODO: it is better to just save the text, and re-evaluate each time, since we do
// need to save things like comments and needed output
struct While {
    condition: String,      // while loop condition, should be valid jq selector
    block: String,          // the entire while block saved to allow looping
    output: String,         // the output of the last run loop, which is returned
}

impl While {
    fn new() -> While {
        While {
            condition: String::new(),
            block: String::new(),
            output: String::new(),
        }
    }

    // TODO: run
}

// TODO: this just builds the entire While loop, if endwhile is found then it
// calls the method call to actually run the While loop
// TODO: NEEDS to find the endwhile block, otherwise return some error
//fn process_while() -> () {
//    // TODO
//}


///////////////////////////////////////////////
/// Unit tests
///////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;

    fn clear_env_file() {
        if let Err(_) = fs::remove_file(ENV_FILE) {
            println!("file doesn't exist")
        } else {
            println!("file deleted")
        }
    }

    //#[test]
    //fn test_process_while() {
    //    // TODO
    //}
}
