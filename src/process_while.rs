/// process_while module
/// Handles while loop for vim-rest-client. A while block is defined thusly:
///
/// ###{ while {{.i < 10}}
/// <requests, variable assignments, folds>
/// ###} endwhile
///
/// The output shown for a while loop should be the result of the final loop.
/// vim-rest-client creates a single SSH session for all connections to the same
/// destination, so if a while loop makes SSH requests, it will reuse that
/// session for all loops.
///
/// Supports nested while loops.

use std::collections::HashMap;
use std::io::BufRead;
use openssh::Session;
use regex::Regex;
use serde_json::{self, Value};

use crate::{parse_input, parse_selectors};

pub const WHILE_START: &str = r"^###\{\s*while\s*(\{\{.*\}\})";
const WHILE_END: &str = r"^###\}\s*endwhile";
const ERROR: &str = r"\(ERROR\)$";

pub struct While {
    condition: String,      // while loop condition, should be valid jq selector
    block: String,          // the entire while block saved to allow looping
    pub output: String,     // the output of the last run loop, which is returned
    pub error: bool,        // error state of the while loop
}

impl While {
    fn new() -> While {
        While {
            condition: String::new(),
            block: String::new(),
            output: String::new(),
            error: false,
        }
    }

    /// Builds the while loop from the input reader, along with the first line
    /// which was already read from the reader by parse_input.
    /// After building the while loop, executes it and returns the struct to
    /// allow the caller to get the error state and output.
    pub fn parse_while(
        first_line: &String,
        input: &mut impl BufRead,
        sessions: &mut HashMap<String, Session>,
        env: &mut Value
    ) -> While {
        let mut w = While::new();
        let mut num_loops = 1;
        let start_re = Regex::new(WHILE_START).unwrap();
        let end_re = Regex::new(WHILE_END).unwrap();
        start_re.captures(first_line)
            .and_then(|caps| caps.get(1))
            .and_then(|condition| {
                w.condition = String::from(condition.as_str());
                Some(())
            });
        if w.condition.is_empty() {
            w.gen_default_output(String::from("Could not get while condition"));
            return w;
        }
        w.block.push_str(first_line);
        w.block.push('\n');
        loop {
            let mut line = String::new();
            let res = input.read_line(&mut line);
            line = String::from((&line).trim_end());
            match res {
                Ok(0) => {
                    break;
                },
                Ok(_) => (),
                Err(e) => {
                    w.error = true;
                    w.output.push_str(&e.to_string());
                    w.gen_default_output(w.output.clone());
                    return w;
                },
            };
            w.block.push_str(&line);
            w.block.push('\n');
            if start_re.is_match(&line) {
                num_loops += 1;
            }
            if end_re.is_match(&line) {
                num_loops -= 1;
            }
            if num_loops == 0 {
                break;
            }
        }
        w.block = String::from(w.block.trim_end());
        w.run(sessions, env);
        w
    }

    /// Run while loop: call parse_input on block while the condition is true
    fn run(&mut self, sessions: &mut HashMap<String, Session>, env: &mut Value) {
        let error_re = Regex::new(ERROR).unwrap();
        while self.check_condition(env) && !self.error {
            // call parse_input with ignore_first_while true to avoid infinite loop
            self.output = parse_input(&mut self.block.clone().as_bytes(), sessions, env, true);
            let first_line = self.output.lines().next().unwrap_or("");
            self.error = self.error || error_re.is_match(first_line);
        }
        if self.output.is_empty() {
            self.gen_default_output(String::new());
        }
    }

    /// Return the block (input) and output of last loop, with proper formatting.
    /// res_input: all lines before ########## marker, and last line
    /// res_output: first line but without { and with only ERROR or RESULT, and
    /// all lines after ########## marker, with last line without }
    pub fn compile_return(&mut self) -> (String, String) {
        let mut res_input = String::new();
        let mut res_output = String::new();
        let first_line = String::from(self.output.lines().next().unwrap_or(""));
        let last_line = self.output.lines().last().unwrap_or("");
        let num_lines = self.output.lines().collect::<Vec<&str>>().len();
        let mut reached_divider = false;
        let suffix_re = Regex::new(r" executed \((ERROR|SUCCESS)\)$").unwrap();

        let first_line_formatted = first_line.replacen("{", "", 1);
        let first_line_formatted = suffix_re.replace(&first_line_formatted, "");
        let first_line_formatted = format!(
            "{} {}",
            first_line_formatted,
            if self.error {"ERROR"} else {"RESULT"}
        );
        let last_line_formatted = last_line.replacen("}", "", 1);
        res_output.push_str(&format!("{}\n", first_line_formatted));
        for (i, line) in self.output.lines().enumerate() {
            if line.starts_with("##########") {
                reached_divider = true;
                continue;
            }
            if i + 1 == num_lines {
                break;
            }
            if !reached_divider {
                res_input.push_str(&format!("{}\n", line));
            } else {
                res_output.push_str(&format!("{}\n", line))
            }
        }
        res_input.push_str(last_line);
        res_output.push_str(&last_line_formatted);
        (res_input, res_output)
    }

    /// Evaluates the condition for the while loop. The jq syntax should return
    /// either true or false.
    fn check_condition(&mut self, env: &mut Value) -> bool {
        parse_selectors(&self.condition, env)
            .map_or_else(
                |err| {
                    self.error = true;
                    self.gen_default_output(err.to_string());
                    false
                },
                |res| res.as_str() == "true"
            )

    }

    /// Creates an output like parse_input, in the case where parse_input wasn't
    /// able to run and it has to be simulated.
    fn gen_default_output(&mut self, output: String) {
        let suffix_re = Regex::new(r" executed \((ERROR|SUCCESS)\)$").unwrap();
        let start_marker_re = Regex::new(r"###\{\s*").unwrap();
        let first_line = String::from(self.block.lines().next().unwrap_or(""));
        let first_line = suffix_re.replace(&first_line, "");
        let title = start_marker_re.replace(&first_line, "");
        let last_line = self.block.lines().last().unwrap_or("");
        let input = self.block.lines().collect::<Vec<&str>>();
        let len = input.len();
        let input = if len > 2 {
            (&input[1..len-1])
                .iter()
                .map(|&l| String::from(l))
                .reduce(|acc, line| format!("{}\n{}", acc, line)).unwrap()
        } else {
            String::new()
        };
        self.output = format!(
            "{} executed ({})\n{}########## {} {}\n{}{}",
            first_line,
            if self.error {"ERROR"} else {"SUCCESS"},
            if input.is_empty() {String::new()} else {format!("{}\n", input)},
            title,
            if self.error {"ERROR"} else {"RESULT"},
            if output.is_empty() {String::new()} else {format!("{}\n", output)},
            last_line
        );
    }
}


///////////////////////////////////////////////
/// Unit tests
///////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use serde_json::json;
    use crate::{ENV_FILE, SshSessions};

    fn clear_env_file() {
        if let Err(_) = fs::remove_file(ENV_FILE) {
            println!("file doesn't exist")
        } else {
            println!("file deleted")
        }
    }

    #[test]
    fn test_while_run() {
        let mut ssh_sessions = SshSessions::new();
        {
            let mut env: Value = json!({
                "i": 0
            });
            let mut test_while = While::new();
            test_while.condition = String::from("{{.i < 5}}");
            test_while.block = String::from(r#"###{ while {{.i < 5}}
@i = {{.i + 1}}
###} endwhile"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let expected = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
########## while {{.i < 5}} RESULT
@i = 5
###} endwhile"#);
            assert_eq!(
                test_while.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                test_while.output
            );
            assert!(!test_while.error);
        }
        {
            let mut env: Value = json!({
                "i": 5
            });
            let mut test_while = While::new();
            test_while.condition = String::from("{{.i < 5}}");
            test_while.block = String::from(r#"###{ while {{.i < 5}}
@i = {{.i + 1}}
###} endwhile"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let expected = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
########## while {{.i < 5}} RESULT
###} endwhile"#);
            assert_eq!(
                test_while.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                test_while.output
            );
            assert!(!test_while.error);
        }
        {
            let mut env: Value = json!({});
            let mut test_while = While::new();
            test_while.condition = String::from("{{.j}}");
            test_while.block = String::from(r#"###{ while {{.j}}
@j = {{.j + 1}}
###} endwhile"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let expected = String::from(r#"###{ while {{.j}} executed (ERROR)
@j = {{.j + 1}}
########## while {{.j}} ERROR
failed to get resource at .j
###} endwhile"#);
            assert_eq!(
                test_while.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                test_while.output
            );
            assert!(test_while.error);
        }

        clear_env_file();
    }

    #[test]
    fn test_compile_return() {
        let mut ssh_sessions = SshSessions::new();
        {
            let mut env: Value = json!({
                "i": 0
            });
            let mut test_while = While::new();
            test_while.condition = String::from("{{.i < 5}}");
            test_while.block = String::from(r#"###{ while {{.i < 5}}
@i = {{.i + 1}}
###} endwhile 1"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let (res_input, res_output) = test_while.compile_return();
            let expected_input = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
###} endwhile 1"#);
            let expected_output = String::from(r#"### while {{.i < 5}} RESULT
@i = 5
### endwhile 1"#);
            assert_eq!(
                res_input,
                expected_input,
                "Expected:\n{}\nGot:\n{}",
                expected_input,
                res_input
            );
            assert_eq!(
                res_output,
                expected_output,
                "Expected:\n{}\nGot:\n{}",
                expected_output,
                res_output
            );
        }
        {
            let mut env: Value = json!({
                "i": 5
            });
            let mut test_while = While::new();
            test_while.condition = String::from("{{.i < 5}}");
            test_while.block = String::from(r#"###{ while {{.i < 5}}
@i = {{.i + 1}}
###} endwhile"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let (res_input, res_output) = test_while.compile_return();
            let expected_input = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
###} endwhile"#);
            let expected_output = String::from(r#"### while {{.i < 5}} RESULT
### endwhile"#);
            assert_eq!(
                res_input,
                expected_input,
                "Expected:\n{}\nGot:\n{}",
                expected_input,
                res_input
            );
            assert_eq!(
                res_output,
                expected_output,
                "Expected:\n{}\nGot:\n{}",
                expected_output,
                res_output
            );
        }
        {
            let mut env: Value = json!({});
            let mut test_while = While::new();
            test_while.condition = String::from("{{.j}}");
            test_while.block = String::from(r#"###{ while {{.j}}
@j = {{.j + 1}}
###} endwhile"#);
            test_while.run(&mut ssh_sessions.sessions, &mut env);
            let (res_input, res_output) = test_while.compile_return();
            let expected_input = String::from(r#"###{ while {{.j}} executed (ERROR)
@j = {{.j + 1}}
###} endwhile"#);
            let expected_output = String::from(r#"### while {{.j}} ERROR
failed to get resource at .j
### endwhile"#);
            assert_eq!(
                res_input,
                expected_input,
                "Expected:\n{}\nGot:\n{}",
                expected_input,
                res_input
            );
            assert_eq!(
                res_output,
                expected_output,
                "Expected:\n{}\nGot:\n{}",
                expected_output,
                res_output
            );
        }

        clear_env_file();
    }

    #[test]
    fn test_parse_while() {
        let mut ssh_sessions = SshSessions::new();
        {
            let mut env: Value = json!({
                "i": 0
            });
            let first_line = String::from("###{ while {{.i < 5}}");
            let input = String::from(r#"@i = {{.i + 1}}
###} endwhile"#);
            let w = While::parse_while(
                &first_line,
                &mut input.as_bytes(),
                &mut ssh_sessions.sessions,
                &mut env
            );
            let expected = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
########## while {{.i < 5}} RESULT
@i = 5
###} endwhile"#);
            assert_eq!(
                w.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                w.output
            );
            assert!(!w.error);
        }
        {
            let mut env: Value = json!({
                "i": 5
            });
            let first_line = String::from("###{ while {{.i < 5}}");
            let input = String::from(r#"@i = {{.i + 1}}
###} endwhile"#);
            let w = While::parse_while(
                &first_line,
                &mut input.as_bytes(),
                &mut ssh_sessions.sessions,
                &mut env
            );
            let expected = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@i = {{.i + 1}}
########## while {{.i < 5}} RESULT
###} endwhile"#);
            assert_eq!(
                w.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                w.output
            );
            assert!(!w.error);
        }
        {
            let mut env: Value = json!({});
            let first_line = String::from("###{ while {{.j}}");
            let input = String::from(r#"@j = {{.j + 1}}
###} endwhile"#);
            let w = While::parse_while(
                &first_line,
                &mut input.as_bytes(),
                &mut ssh_sessions.sessions,
                &mut env
            );
            let expected = String::from(r#"###{ while {{.j}} executed (ERROR)
@j = {{.j + 1}}
########## while {{.j}} ERROR
failed to get resource at .j
###} endwhile"#);
            assert_eq!(
                w.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                w.output
            );
            assert!(w.error);
        }
        {
            let mut env: Value = json!({
                "i": 0,
                "n": 0
            });
            let first_line = String::from("###{ while {{.i < 5}}");
            let input = String::from(r#"@j = 0
###{ while {{.j < 3}}
@j = {{.j + 1}}
@n = {{.n + 1}}
###} endwhile
@i = {{.i + 1}}
###} endwhile"#);
            let w = While::parse_while(
                &first_line,
                &mut input.as_bytes(),
                &mut ssh_sessions.sessions,
                &mut env
            );
            let expected = String::from(r#"###{ while {{.i < 5}} executed (SUCCESS)
@j = 0
###{ while {{.j < 3}} executed (SUCCESS)
@j = {{.j + 1}}
@n = {{.n + 1}}
###} endwhile
@i = {{.i + 1}}
########## while {{.i < 5}} RESULT
@j = 0
### while {{.j < 3}} RESULT
@j = 3
@n = 15
### endwhile
@i = 5
###} endwhile"#);
            assert_eq!(
                w.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                w.output
            );
            assert!(!w.error);
        }
        {
            let mut env: Value = json!({
                "i": 0
            });
            let first_line = String::from("###{ while {{.i < 5}}");
            let input = String::from(r#"@i = {.i + 1}
###} endwhile"#);
            let w = While::parse_while(
                &first_line,
                &mut input.as_bytes(),
                &mut ssh_sessions.sessions,
                &mut env
            );
            let expected = String::from(r#"###{ while {{.i < 5}} executed (ERROR)
@i = {.i + 1}
########## while {{.i < 5}} ERROR
key must be a string at line 1 column 2
###} endwhile"#);
            assert_eq!(
                w.output,
                expected,
                "Expected:\n{}\nGot:\n{}",
                expected,
                w.output
            );
            assert!(w.error);
        }

        clear_env_file();
    }
}
