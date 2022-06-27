/// Vim REST Client helper script.
/// Parses output filtered from the .rest file by Vim.
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, BufRead};
use std::ops::{Deref, DerefMut};
use std::process::Command;

use base64::encode;
use jq_rs;
use openssh::{Session, SessionBuilder};
use regex::{Regex, Captures};
use serde_json::{self, Value, json};
use tokio::runtime::Runtime;

pub mod process_while;

// TODO: perhaps configurable location by ENV variable
// TODO: or maybe the env should be based on the file name, like .file.rest.json
pub const ENV_FILE: &str = ".env.json";

// SSH config vars
const SSH_TO: &str = "sshTo";
const SSH_CONFIG: &str = "sshConfig";
const SSH_KEY: &str = "sshKey";

#[derive(Clone)]
enum Method {
    Get,
    Post,
    Delete,
    Put,
    Other(String)
}

impl Method {
    fn get_match(s: &str) -> Method {
        match s.to_lowercase().as_str() {
            "get" => Method::Get,
            "post" => Method::Post,
            "delete" => Method::Delete,
            "put" => Method::Put,
            _ => Method::Other(s.to_uppercase()),
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let method_str = match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Delete => "DELETE",
            Method::Put => "PUT",
            Method::Other(s) => &s,
        };
        write!(f, "{}", method_str)
    }
}

struct Request {
    method: Method,
    url: String,
    headers: Vec<String>,
    data: Option<String>,
    multipart_forms: Vec<String>,
}

impl Request {
    /// Calls curl with appropriate args to make the desired request
    /// Substitutions can happen with {{}} and a variable name, or jq-syntax for
    /// selecting fields from a variable.
    /// Return the response headers and response body (pretty-printed, if JSON),
    /// or the error with error cause if curl failed.
    /// (String, Value) = (entire response string with headers, just response)
    fn make_request
    (
        &self,
        g_env: &mut GlobalEnv,
        is_debug: bool,
        is_verbose: bool,
    ) -> Result<(String, Value), Box<dyn Error>> {
        let method = self.method.to_string();
        let url = g_env.parse_selectors(&self.url)?;
        let mut header_err: Option<String> = None;
        let basic_auth_re = Regex::new(r"^(Authorization:\s+Basic\s+)([^:]+:[^:]+)$").unwrap();
        let headers = self.headers.iter().map(|header| {
            g_env.parse_selectors(header)
                .map_or_else(
                    |e| {
                        header_err = Some(e.to_string());
                        String::from("ERR")
                    },
                    |replaced| handle_basic_auth(replaced, &basic_auth_re)
                )
        }).collect::<Vec<String>>();
        let multipart_forms = self.multipart_forms.iter().map(|form| {
            g_env.parse_selectors(form)
                .map_or_else(
                    |e| {
                        header_err = Some(e.to_string());
                        String::from("ERR")
                    },
                    |replaced| replaced
                )
        }).collect::<Vec<String>>();
        if let Some(e) = &header_err {
            return Err(io_error(&e))?;
        }
        let data = if let Some(data) = &self.data {
            Some(g_env.parse_selectors(&data)?)
        } else {
            None
        };
        let mut args = vec!["-k", if is_verbose {"-v"} else {"--include"}, &url, "-X", &method]
            .iter()
            .map(|&s| String::from(s))
            .collect::<Vec<String>>();
        for header in headers {
            args.push(String::from("-H"));
            args.push(String::from(header));
        }
        if let Some(d) = data {
            args.push(String::from("-d"));
            args.push(String::from(d));
        }
        for form in multipart_forms {
            args.push(String::from("-F"));
            args.push(String::from(form));
        }
        if is_debug {
            args.insert(0, String::from("curl"));
            return Ok((args.join(" "), json!("")));
        }
        let (ret, e) = g_env.call_curl(&args)?;

        enum Response {
            NoSplit(String), // whole response
            NonJson(String, String), // headers, response
            Json(String, Value), // headers, JSON response
        }
        impl Response {
            fn get_return(self) -> (String, Value) {
                match self {
                    Response::NoSplit(response) => (response, json!("")),
                    Response::NonJson(headers, resp) => (format!("{}\n\n{}", headers, resp), json!(resp)),
                    Response::Json(headers, val) => {
                        let print_json: String = serde_json::to_string_pretty(&val)
                            .or::<String>(Ok(val.to_string()))
                            .unwrap();
                        (format!("{}\n\n{}", headers, print_json), val)
                    },
                }
            }
        }
        // if verbose, return is from stdout, and the other output is stderr
        let mut ret_enum = if is_verbose {
            Response::NonJson(String::from(&e), String::from(ret))
        } else {
            ret.split_once("\n\n")
                .map_or_else(
                    || Response::NoSplit(String::from(&ret)),
                    |(headers, resp)| Response::NonJson(String::from(headers), String::from(resp)))
        };
        if let Response::NonJson(headers, resp) = ret_enum {
            ret_enum = serde_json::from_str::<Value>(&resp)
                .map_or_else(
                    |_| Response::NonJson(String::from(&headers), String::from(&resp)),
                    |r_json| Response::Json(String::from(&headers), r_json));
        }
        Ok(ret_enum.get_return())
    }
}

/// Given a header string, if it is for basic auth then automatically convert
/// the user:pass string to base64, as appropriate. Returns the original string
/// if not.
fn handle_basic_auth(header: String, basic_auth_re: &Regex) -> String {
    basic_auth_re.replace(&header, |caps: &Captures| {
        format!("{}{}", &caps[1], encode(&caps[2].as_bytes()))
    }).to_string()
}


/// Variables related to executing the content of a single fold
struct FoldEnv {
    ret: String,                        // returned input
    output: String,                     // returned executed output
    title: String,                      // title of fold
    start_marker: String,               // start of fold, without "executed" text
    end_marker: String,                 // end of fold, in case there is a comment added
    error: bool,                        // if error occurred during execution
    first_line: bool,                   // if the first line has occurred yet
    old_output_started: bool,           // if the output from previous execution was reached
    compiled: bool,                     // if this FoldEnv has compiled the return
    parent_fold: Option<Box<FoldEnv>>,  // if this FoldEnv is nested, contains the parent

    // request related vars
    request_started: bool,              // if the fold has started defining a request
    request_body_started: bool,         // if the fold has started the request body
    response_variable: String,          // variable to store the response
    made_request: bool,                 // if the request was made
    method: Method,                     // request method
    url: String,                        // request url
    headers: Vec<String>,               // request headers
    multipart_forms: Vec<String>,       // forms and form data for multipart forms
    request_body: String,               // request body
    is_debug: bool,                     // is debug flag set
    is_verbose: bool,                   // is verbose flag set
}

impl FoldEnv {
    fn new() -> FoldEnv {
        FoldEnv {
            ret: String::new(),
            output: String::new(),
            title: String::new(),
            start_marker: String::new(),
            end_marker: String::new(),
            error: false,
            first_line: true,
            old_output_started: false,
            compiled: false,
            parent_fold: None,

            request_started: false,
            request_body_started: false,
            response_variable: String::new(),
            made_request: false,
            method: Method::Get,
            url: String::new(),
            headers: Vec::new(),
            multipart_forms: Vec::new(),
            request_body: String::new(),
            is_debug: false,
            is_verbose: false,
        }
    }

    /// Collects the total string to return, including input and output
    fn compile_return(&mut self) -> String {
        if !self.compiled && !self.ret.is_empty() {
            self.compiled = true;
            let mut ret = String::new();
            ret.push_str(&format!("{} executed ({})\n", self.start_marker,
                if self.error {"ERROR"} else {"SUCCESS"}));
            ret.push_str(&self.ret);
            insert_newline(&mut ret);
            ret.push_str(&format!("########## {}{}\n",
                self.title,
                if self.error {"ERROR"} else {"RESULT"}));
            insert_newline(&mut self.output);
            if self.end_marker.is_empty() {
                self.output.push_str("###}");
            } else {
                self.output.push_str(&self.end_marker);
            }
            ret.push_str(&self.output);
            ret
        } else {
            String::new()
        }
    }

    /// Collects the total string to return, including input and output
    fn compile_for_parent(&mut self) -> (String, String) {
        if !self.compiled && self.parent_fold.is_some() {
            self.compiled = true;
            let mut ret = String::new();
            let mut out = String::new();
            ret.push_str(&format!("{} executed ({})\n", self.start_marker,
                if self.error {"ERROR"} else {"SUCCESS"}));
            ret.push_str(&self.ret);
            if self.end_marker.is_empty() {
                ret.push_str("###}");
            } else {
                ret.push_str(&self.end_marker);
            }
            ret.push('\n');
            let parent_out = &self.parent_fold.as_ref().unwrap().output;
            if !parent_out.is_empty() && parent_out.chars().last().unwrap() != '\n' {
                out.push('\n');
            }
            out.push_str(&format!("### {}{}\n",
                self.title,
                if self.error {"ERROR"} else {"RESULT"}));
            insert_newline(&mut self.output);
            out.push_str(&self.output);
            out.push_str("###\n");
            (ret, out)
        } else {
            (String::new(), String::new())
        }
    }

    /// Builds and makes request if appropriate
    fn make_request(&mut self, g_env: &mut GlobalEnv) {
        if self.request_started && !self.error {
            let method = self.method.clone();
            let url = self.url.clone();
            let headers = self.headers.clone();
            let multipart_forms = self.multipart_forms.clone();
            let req = Request {
                method,
                url,
                headers,
                multipart_forms,
                data: if self.request_body_started {
                    Some(self.request_body.clone())
                } else {
                    None
                },
            };
            self.made_request = true;
            req.make_request(g_env, self.is_debug, self.is_verbose)
                .and_then(|(response, val)| {
                    if !self.response_variable.is_empty() {
                        let res = g_env.set_var(&self.response_variable, &val);
                        if let Err(_) = res {
                            return res;
                        }
                    }
                    self.output.push_str(&response);
                    Ok(())
                })
                .or_else(|err| -> Result<(), ()>{
                    self.error = true;
                    self.output.push_str(&format!("{}\n", err.to_string()));
                    Ok(())
                }).unwrap();
        }
    }

    /// Parses flags
    fn parse_flags(&mut self, line: &String, flags: &Flags) {
        // check for # @name <name> which will do a variable definition on the response
        flags.response_var_re.captures(line)
            .and_then(|caps| caps.get(1))
            .and_then(|var_name| {
                self.response_variable = String::from(var_name.as_str());
                Some(())
            });
        // check for # @form <form assign> which adds a multipart form arg
        // <form assign> has the syntax
        // - form_name=form_value
        // - form_name=@file_path
        flags.multi_form_re.captures(line)
            .and_then(|caps| caps.get(1))
            .and_then(|form| {
                self.multipart_forms.push(String::from(form.as_str()));
                Some(())
            });
        // check for # @debug which will print the curl request rather than run it
        if flags.debug_re.is_match(line) {
            self.is_debug = true;
        }
        // check for # @verbose which will run curl with verbose flag
        if flags.verbose_re.is_match(line) {
            self.is_verbose = true;
        }
    }
}

pub struct SshSessions {
    pub sessions: HashMap<String, Session>,
}

impl SshSessions {
    pub fn new() -> SshSessions {
        SshSessions {
            sessions: HashMap::new(),
        }
    }

    async fn close_sessions(&mut self) {
        for (_, session) in self.sessions.drain() {
            session.close().await.unwrap();
        }
    }
}

impl Drop for SshSessions {
    fn drop(&mut self) {
        let rt = Runtime::new().unwrap();
        rt.block_on(self.close_sessions());
    }
}

impl Deref for SshSessions {
    type Target = HashMap<String, Session>;

    fn deref(&self) -> &Self::Target {
        &self.sessions
    }
}

impl DerefMut for SshSessions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sessions
    }
}

/// Flags that are indicated with a syntax like so:
/// # @flag_name
pub struct Flags {
    response_var_re: Regex,
    multi_form_re: Regex,
    debug_re: Regex,
    verbose_re: Regex,
}

impl Flags {
    fn new() -> Flags {
        Flags {
            response_var_re: Regex::new(r"^#\s*@name\s*([^ ]+)").unwrap(),
            multi_form_re: Regex::new(r"^#\s*@form\s*(.+=.+)").unwrap(),
            debug_re: Regex::new(r"^#\s*@debug").unwrap(),
            verbose_re: Regex::new(r"^#\s*@verbose").unwrap(),
        }
    }
}

/// Global environment that contains the sessions map and env variables map.
pub struct GlobalEnv {
    pub sessions: SshSessions,
    pub env: Value,
}

impl GlobalEnv {
    pub fn new() -> GlobalEnv {
        GlobalEnv {
            sessions: SshSessions::new(),
            env: fs::read_to_string(ENV_FILE)
                .and_then(|env_string| serde_json::from_str(&env_string)
                      .or_else(|e| Err(io_error(&e.to_string()))))
                .map_or_else(|_| json!({}), |val| val)
        }
    }

    /// Parse input lines that either define a variable or make a request
    /// Must return the input lines, as well as appropriate output
    /// Each block can have some variable definitions, but they must be before the
    /// request. The request starts with the method, and it is assumed the rest of
    /// the lines of the block are the headers of the request.
    pub fn parse_input
    (
        &mut self,
        input: &mut impl BufRead,
        ignore_first_while: bool,
    ) -> String {
        let mut fold_env = FoldEnv::new();
        let mut ret = String::new();
        let mut fold_started = false;

        let start_fold_re = Regex::new(r"^(###\{\s*(.*))$").unwrap();
        let executed_re = Regex::new(r" ?executed( \((ERROR|SUCCESS)\))?$").unwrap();
        let while_re = Regex::new(process_while::WHILE_START).unwrap();
        let flags = Flags::new();
        let mut first_while = true;
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
                    fold_env.error = true;
                    fold_env.output.push_str(&e.to_string());
                },
            };
            let start_while = while_re.is_match(&line);
            if start_while && !(ignore_first_while && first_while) {
                let mut w = process_while::While::parse_while(&line, input, self);
                if fold_started {
                    let (nest_ret, nest_out) = w.compile_return();
                    fold_env.ret.push_str(&nest_ret);
                    fold_env.output.push_str(&nest_out);
                    fold_env.error = fold_env.error || w.error;
                } else {
                    ret.push_str(&w.output);
                }
                first_while = false;
                continue;
            } else if start_while {
                first_while = false;
            }
            if let Some(caps) = start_fold_re.captures(&line) {
                if !fold_started {
                    // previous endmarker doesn't end with newline
                    if !ret.is_empty() {
                        ret.push('\n');
                    }
                    fold_started = true;
                    fold_env = FoldEnv::new();
                } else {
                    // if creating a new nested_fold, then check for request and run it
                    if !fold_env.made_request {
                        fold_env.make_request(self);
                    }
                    let mut nested_fold = FoldEnv::new();
                    nested_fold.parent_fold = Some(Box::new(fold_env));
                    fold_env = nested_fold;
                }
                if let Some(res) = caps.get(2) {
                    let no_exec = executed_re.replace(res.as_str(), "");
                    if !no_exec.to_string().is_empty() {
                        fold_env.title = format!("{} ", no_exec.to_string());
                    }
                }
                if let Some(res) = caps.get(1) {
                    let no_exec = executed_re.replace(res.as_str(), "");
                    fold_env.start_marker = no_exec.to_string();
                } else {
                    fold_env.start_marker = String::from("###{");
                }
                fold_env.first_line = false;
                continue;
            } else if fold_env.first_line && fold_started {
                fold_env.start_marker = String::from("###{");
                fold_env.first_line = false;
            } else if !fold_started {
                // push stuff in between folds
                if !ret.is_empty() {
                    ret.push('\n');
                }
                ret.push_str(&line);
            }
            if !fold_started {
                continue;
            }
            if line.starts_with("##########") && fold_started {
                fold_env.old_output_started = true;
                continue;
            }
            if line.starts_with("###}") {
                fold_env.end_marker = String::from(&line);
                if !fold_env.made_request {
                    fold_env.make_request(self);
                }
                if fold_env.parent_fold.is_some() {
                    let (nest_ret, nest_out) = &fold_env.compile_for_parent();
                    fold_env.parent_fold.as_mut().unwrap().ret.push_str(&nest_ret);
                    fold_env.parent_fold.as_mut().unwrap().output.push_str(&nest_out);
                    let mut parent_err = fold_env.parent_fold.as_mut().unwrap().error;
                    parent_err = fold_env.error || parent_err;
                    fold_env = *fold_env.parent_fold.take().unwrap();
                    fold_env.error = parent_err;
                } else {
                    ret.push_str(&fold_env.compile_return());
                    fold_started = false;
                }
                continue;
            }
            if fold_env.old_output_started {
                continue;
            }
            insert_newline(&mut fold_env.ret);
            fold_env.ret.push_str(&line);
            fold_env.ret.push('\n');
            if fold_env.error {
                continue;
            }
            if line.starts_with('@') {
                // for each line that starts with @, call define_var
                let res_line = self.define_var(&String::from(line))
                    .map_or_else(
                        |err| {
                            fold_env.error = true;
                            format!("{}\n", err.to_string())
                        },
                        |res| format!("{}\n", res)
                    );
                insert_newline(&mut fold_env.output);
                fold_env.output.push_str(&res_line);
            } else if line.starts_with('#') {
                // parse and check flags, else skip comment
                fold_env.parse_flags(&line, &flags);
            } else if !fold_env.request_started && line.is_empty() {
                // line breaks should be ignored, but appear in output
                fold_env.output.push('\n');
                continue;
            } else if !fold_env.request_started {
                // parse method and URL
                line.split_once(' ')
                    .map_or_else(
                        || {
                            fold_env.error = true;
                            insert_newline(&mut fold_env.output);
                            fold_env.output.push_str(&format!("Could not parse line: {}\n", line));
                            ()
                        },
                        |(m, url_str)| {
                            fold_env.made_request = false;
                            fold_env.method = Method::get_match(m);
                            fold_env.url = String::from(url_str);
                            ()
                        }
                    );
                fold_env.request_started = true;
            } else if !fold_env.request_body_started && !line.is_empty() {
                fold_env.headers.push(String::from(line));
            } else if !fold_env.request_body_started && line.is_empty() {
                fold_env.request_body_started = true
            } else if fold_env.request_body_started {
                fold_env.request_body.push_str(&line);
            }
        }

        if !fold_env.made_request {
            fold_env.make_request(self);
            ret.push_str(&fold_env.compile_return());
        }

        ret
    }

    /// Defines and stores a variable (one line)
    /// Parse the variable value as JSON, since the storage will basically be a JSON
    /// file at .env.json. Should update both the file and the JSON loaded by
    /// parse_input.
    /// Substitutions can happen with {{}} and a variable name, or jq-syntax for
    /// selecting fields from a variable.
    /// If there's an error, return the error with error cause.
    /// If successful, return the line with the value stored, with substitutions.
    fn define_var(&mut self, var_line: &String) -> Result<String, Box<dyn Error>> {
        let re = Regex::new(r"@([^ ]+)\s*=\s*(.+)").unwrap();
        let caps = re.captures(var_line)
            .ok_or(io_error(&format!("cannot parse line: {}", var_line)))?;
        let var_name = caps.get(1).ok_or(io_error("unable to get variable"))?;
        let value = caps.get(2).ok_or(io_error("unable to get value"))?;

        let value = self.parse_selectors(&String::from(value.as_str()))?;
        let value_json = serde_json::from_str(&value)?;
        self.set_var(&String::from(var_name.as_str()), &value_json)?;
        Ok(format!("@{} = {}", var_name.as_str(), value))
    }

    /// Given a variable and value, add it to the env and set file.
    fn set_var(&mut self, var: &String, val: &Value) -> Result<(), Box<dyn Error>> {
        self.env.as_object_mut()
            .ok_or(io_error("cannot modify environment"))?
            .insert(String::from(var), val.clone());
        fs::write(ENV_FILE, serde_json::to_string_pretty(&self.env)?)?;
        Ok(())
    }

    /// Given a string, parses the entire string for substitutions marked by any
    /// selectors in {{}}. If there are none, the original string is returned.
    /// Allow substitutions to be nested.
    pub fn parse_selectors(&mut self, s: &String) -> Result<String, Box<dyn Error>> {
        let re = Regex::new(r"\{\{([^{}]+)\}\}").unwrap();
        let mut replace_err: Option<String> = None;
        let value = re.replace_all(s.as_str(), |caps: &Captures| {
            let selector = caps.get(1);
            if let None = selector {
                replace_err = Some(String::from("unable to get selector"));
                return String::from("ERR");
            }
            let selector = selector.unwrap();
            let selector_val = self.evaluate(&String::from(selector.as_str()));
            if let Err(err) = selector_val {
                replace_err = Some(err.to_string());
                return String::from("ERR");
            }
            let selector_val = selector_val.unwrap();
            selector_val.as_str()
                .map_or_else(
                    || selector_val.to_string(),
                    |s| String::from(s)
                )
        });
        if let Some(err) = replace_err {
            return Err(io_error(&err))?;
        }
        let subbed = value.to_string();
        if re.is_match(&subbed) {
            return self.parse_selectors(&subbed);
        }
        Ok(subbed)
    }

    /// Given a particular string representing a variable or jq selection, evaluate
    /// the value in the environment json. If there's an error, return the error
    /// with the error cause. Due to jq returning null for out-of-bounds or no key,
    /// this function will have a generic null error message.
    /// If the selector string represents an environment variable (like $VAR) then
    /// retrieve the value from the appropriate environment and return a json string.
    fn evaluate(&mut self, selector: &String) -> Result<Value, Box<dyn Error>> {
        if let Some(val) = self.get_env_var(selector)? {
            return Ok(val);
        }
        let res_str = jq_rs::run(&selector, &self.env.to_string())?;
        let res_val = serde_json::from_str(&res_str)?;
        match res_val {
            Value::Null => Err(io_error(&format!("failed to get resource at {}", selector)))?,
            _ => Ok(res_val)
        }
    }

    /// Given a selector, checks if it has the pattern for an environment variable,
    /// like $VAR. If not, return None, otherwise return the value of the env var if
    /// it exists, or an empty string. If sshTo is defined, then retrieve the
    /// environment variable on the desired machine.
    fn get_env_var
    (
        &mut self,
        selector: &String,
    ) -> Result<Option<Value>, Box<dyn Error>> {
        let env_var_re = Regex::new(r"^\$(.*)$").unwrap();
        if let Some(caps) = env_var_re.captures(selector) {
            let var = caps.get(1).unwrap().as_str();
            if let Some(_) = self.env.get(SSH_TO) {
                let rt = Runtime::new()?;
                let val = rt.block_on(self.ssh_get_env_var(&String::from(selector)))?;
                return Ok(Some(val));
            }
            Ok(env::var(var)
                .map_or_else(|_| Some(json!("")), |val| Some(json!(val))))
        } else {
            Ok(None)
        }
    }

    fn call_curl(&mut self, args: &Vec<String>) -> Result<(String, String), Box<dyn Error>> {
        if let Some(_) = self.env.get(SSH_TO) {
            let rt = Runtime::new()?;
            return rt.block_on(self.ssh_curl(args));
        }
        let curl = Command::new("curl")
            .args(args)
            .output()?;
        let e = String::from_utf8_lossy(&curl.stderr).to_string();
        if !curl.status.success() {
            return Err(io_error(&e))?;
        }
        let ret = String::from_utf8_lossy(&curl.stdout).to_string();
        let ret = ret.replace('\r', "");
        let e = e.replace('\r', "");
        Ok((ret, e))
    }

    async fn ssh_curl(&mut self, args: &Vec<String>) -> Result<(String, String), Box<dyn Error>> {
        let dest = self.env.get(SSH_TO)
            .unwrap()
            .as_str()
            .ok_or_else(|| io_error(&format!("{} was not a string", SSH_TO)))?;
        let session = if let Some(sess_ref) = self.sessions.remove(dest) {
            sess_ref
        } else {
            let mut session_builder = SessionBuilder::default();
            if let Some(config) = self.env.get(SSH_CONFIG) {
                let config = config.as_str().ok_or_else(|| io_error(&format!("{} was not a string", SSH_CONFIG)))?;
                session_builder.config_file(config);
            }
            if let Some(key) = self.env.get(SSH_KEY) {
                let key = key.as_str().ok_or_else(|| io_error(&format!("{} was not a string", SSH_KEY)))?;
                session_builder.keyfile(key);
            }
            session_builder.connect_mux(dest).await?
        };
        let curl = session.command("curl")
            .args(args)
            .output()
            .await?;
        let e = String::from_utf8_lossy(&curl.stderr).to_string();
        if !curl.status.success() {
            return Err(io_error(&e))?;
        }
        let ret = String::from_utf8_lossy(&curl.stdout).to_string();
        let ret = ret.replace('\r', "");
        let e = e.replace('\r', "");
        self.sessions.insert(String::from(dest), session);
        Ok((ret, e))
    }

    async fn ssh_get_env_var(&mut self, var: &String) -> Result<Value, Box<dyn Error>> {
        let dest = self.env.get(SSH_TO)
            .unwrap()
            .as_str()
            .ok_or_else(|| io_error(&format!("{} was not a string", SSH_TO)))?;
        let session = if let Some(sess_ref) = self.sessions.remove(dest) {
            sess_ref
        } else {
            let mut session_builder = SessionBuilder::default();
            if let Some(config) = self.env.get(SSH_CONFIG) {
                let config = config.as_str().ok_or_else(|| io_error(&format!("{} was not a string", SSH_CONFIG)))?;
                session_builder.config_file(config);
            }
            if let Some(key) = self.env.get(SSH_KEY) {
                let key = key.as_str().ok_or_else(|| io_error(&format!("{} was not a string", SSH_KEY)))?;
                session_builder.keyfile(key);
            }
            session_builder.connect_mux(dest).await?
        };
        let echo = session.command("echo")
            .raw_arg(var)
            .output()
            .await?;
        let e = String::from_utf8_lossy(&echo.stderr).to_string();
        if !echo.status.success() {
            return Err(io_error(&e))?;
        }
        let ret = String::from_utf8_lossy(&echo.stdout).to_string();
        let ret = ret.replace('\r', "");
        let ret = ret.replace('\n', "");
        self.sessions.insert(String::from(dest), session);
        Ok(json!(ret))
    }
}


/// Returns an error
pub fn io_error(err: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

/// Adds a newline to the string if the last char is not a newline
fn insert_newline(s: &mut String) {
    if !s.is_empty() && s.chars().last().unwrap() != '\n' {
        s.push('\n');
    }
}


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

    #[test]
    fn test_parse_selectors() {
        // create dummy env (json) and call evaluate to see if it returns the
        // right values
        let mut g_env = GlobalEnv::new();
        g_env.env = json!({
            "arr": ["a", "b", "c"],
            "str": "value",
            "num": 1,
            "bool": true,
            "obj": {"a": 1, "b": 2},
            "a": "test",
            "a1": "success"
        });

        {
            let s = String::from("\"Some String\"");
            let res = g_env.parse_selectors(&s).unwrap();
            assert_eq!(res, s, "Expected {}, but got {}", s, res);
        }
        {
            let s = String::from("\"Some {{.str}}\"");
            let res = g_env.parse_selectors(&s).unwrap();
            let expect = String::from("\"Some value\"");
            assert_eq!(res, expect, "Expected {}, but got {}", expect, res);
        }
        {
            let s = String::from("\"{{.obj.{{.arr[0]}}}}\"");
            let res = g_env.parse_selectors(&s).unwrap();
            let expect = String::from("\"1\"");
            assert_eq!(res, expect, "Expected {}, but got {}", expect, res);
        }
        {
            let s = String::from("\"{{.{{.arr[0]}}}}\"");
            let res = g_env.parse_selectors(&s).unwrap();
            let expect = String::from("\"test\"");
            assert_eq!(res, expect, "Expected {}, but got {}", expect, res);
        }
        {
            let s = String::from("\"{{.{{.arr[0]}}{{.num}}}}\"");
            let res = g_env.parse_selectors(&s).unwrap();
            let expect = String::from("\"success\"");
            assert_eq!(res, expect, "Expected {}, but got {}", expect, res);
        }
    }

    #[test]
    fn test_evaluate() {
        // create dummy env (json) and call evaluate to see if it returns the
        // right values
        let mut g_env = GlobalEnv::new();
        g_env.env = json!({
            "arr": ["a", "b", "c"],
            "str": "value",
            "num": 1,
            "bool": true,
            "obj": {"a": 1, "b": 2}
        });
        {
            let arr = g_env.evaluate(&String::from(".arr")).unwrap();
            assert_eq!(arr, json!(["a", "b", "c"]), "Expected [\"a\", \"b\", \"c\"], but got {:?}", arr);
            let arr0 = g_env.evaluate(&String::from(".arr[0]")).unwrap();
            assert_eq!(arr0, json!("a"), "Expected \"a\", but got {:?}", arr0);
            let arr_err = g_env.evaluate(&String::from(".arr[3]"));
            match arr_err {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "failed to get resource at .arr[3]",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let strng = g_env.evaluate(&String::from(".str")).unwrap();
            assert_eq!(strng, json!("value"), "Expected \"value\", but got {:?}", strng);
            let num = g_env.evaluate(&String::from(".num")).unwrap();
            assert_eq!(num, json!(1), "Expected 1, but got {:?}", num);
            let boolean = g_env.evaluate(&String::from(".bool")).unwrap();
            assert_eq!(boolean, json!(true), "Expected true, but got {:?}", boolean);
        }
        {
            let obj = g_env.evaluate(&String::from(".obj")).unwrap();
            assert_eq!(obj, json!({"a": 1, "b": 2}), "Expected {{\"a\": 1, \"b\", 2}}, but got {:?}", obj);
            let obj_a = g_env.evaluate(&String::from(".obj.a")).unwrap();
            assert_eq!(obj_a, json!(1), "Expected 1, but got {:?}", obj_a);
            let obj_err = g_env.evaluate(&String::from(".obj.c"));
            match obj_err {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "failed to get resource at .obj.c",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let dne = g_env.evaluate(&String::from(".DNE_KEY"));
            match dne {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "failed to get resource at .DNE_KEY",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let env_var = g_env.evaluate(&String::from("$SHELL")).unwrap();
            assert_eq!(env_var, json!("/bin/bash"), "Expected \"/bin/bash\", but got {:?}", env_var);
            let dne_env_var = g_env.evaluate(&String::from("$DNE_VAR")).unwrap();
            assert_eq!(dne_env_var, json!(""), "Expected \"\", but got {:?}", dne_env_var);
        }
    }

    #[test]
    fn test_define_var() {
        let mut g_env = GlobalEnv::new();
        g_env.env = json!({"init": "test"});
        fn verify_sub(var: &str, in_val: &str, sub_val: &str, g_env: &mut GlobalEnv) {
            let test_in = format!("@{} = {}", var, in_val);
            let test_out = format!("@{} = {}", var, sub_val);
            println!("in: {}", test_in);
            let out = g_env.define_var(&test_in).unwrap();
            assert_eq!(out, test_out, "Expected \"{}\", but got \"{}\"", test_out, out);
            let check = g_env.evaluate(&format!(".{}", var)).unwrap();
            let expect: Value = serde_json::from_str(sub_val).unwrap();
            assert_eq!(check, expect, "Expected {:?}, got {:?}", expect, check);
        }
        fn verify_non_sub(var: &str, val: &str, g_env: &mut GlobalEnv) {
            let test_in = format!("@{} = {}", var, val);
            println!("in: {}", test_in);
            let out = g_env.define_var(&test_in).unwrap();
            assert_eq!(out, test_in, "Expected \"{}\", but got \"{}\"", test_in, out);
            let check = g_env.evaluate(&format!(".{}", var)).unwrap();
            let expect: Value = serde_json::from_str(val).unwrap();
            assert_eq!(check, expect, "Expected {:?}, got {:?}", expect, check);
        }

        {
            verify_non_sub("baseUrl", "\"https://10.0.0.20:5443/api/v1\"", &mut g_env);
        }
        {
            verify_non_sub("urls", "[\"https://10.0.0.20:5443/api/v1\", \"https://reqbin.com\"]", &mut g_env);
            verify_non_sub("obj", "{\"a\": \"test\", \"b\": \"hello\"}", &mut g_env);
            verify_non_sub("int1", "50", &mut g_env);
        }
        {
            fn check_env_file() -> Result<(), Box<dyn Error>> {
                let file_str = fs::read_to_string(ENV_FILE)?;
                assert!(file_str.contains("baseUrl"), "File should contain baseUrl");
                assert!(!file_str.contains("fail"), "File should not contain fail");
                Ok(())
            }
            if let Err(e) = check_env_file() {
                panic!("Got error: {}", e.to_string());
            }
        }
        {
            let fail_err = g_env.define_var(&String::from("@fail = some invalid json"));
            match fail_err {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "expected value at line 1 column 1",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let fail_err = g_env.define_var(&String::from("@fail \"line invalid\""));
            match fail_err {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "cannot parse line: @fail \"line invalid\"",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            verify_sub("testUrl", "\"{{.baseUrl}}/test\"", "\"https://10.0.0.20:5443/api/v1/test\"", &mut g_env);
            verify_sub("url1", "\"{{.urls[0]}}\"", "\"https://10.0.0.20:5443/api/v1\"", &mut g_env);
            verify_sub("objA", "\"{{.obj.a}}\"", "\"test\"", &mut g_env);
            verify_sub("objB", "\"{{.baseUrl}}/{{.obj.b}}\"", "\"https://10.0.0.20:5443/api/v1/hello\"", &mut g_env);
        }
        {
            let test_fail_sub = r#"@fail = "{{.dne}}""#;
            let fail_err = g_env.define_var(&String::from(test_fail_sub));
            match fail_err {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "failed to get resource at .dne",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        clear_env_file();
    }

    #[test]
    fn test_make_request() {
        let mut g_env = GlobalEnv::new();
        g_env.env = json!({
            "baseUrl": "https://reqbin.com",
            "getXml": "echo/get/xml",
            "ct": "Content-Type",
            "json": "application/json"
        });
        {
            let req = Request {
                method: Method::Get,
                url: String::from("https://reqbin.com/echo/get/xml"),
                headers: vec![],
                multipart_forms: vec![],
                data: None,
            };
            let (resp, val) = req.make_request(&mut g_env, false, false).unwrap();
            let expected = "<?xml version=\"1.0\" encoding=\"utf-8\"?><Response>  <ResponseCode>0</ResponseCode>  <ResponseMessage>Success</ResponseMessage></Response>";
            let resp = resp.lines().last().unwrap();
            assert_eq!(resp, expected, "Expected {}, got {}", expected, resp);
            assert!(val.is_string(), "Response is XML so value should be string, got {:?}", val);
        }
        {
            let req = Request {
                method: Method::Get,
                url: String::from("{{.baseUrl}}/{{.getXml}}"),
                headers: vec![],
                multipart_forms: vec![],
                data: None,
            };
            let (resp, _) = req.make_request(&mut g_env, false, false).unwrap();
            let expected = "<?xml version=\"1.0\" encoding=\"utf-8\"?><Response>  <ResponseCode>0</ResponseCode>  <ResponseMessage>Success</ResponseMessage></Response>";
            let resp = resp.lines().last().unwrap();
            assert_eq!(resp, expected, "Expected {}, got {}", expected, resp);
        }
        {
            let req = Request {
                method: Method::Post,
                url: String::from("https://reqbin.com/echo/post/json"),
                headers: vec![String::from("{{.ct}}: {{.json}}")],
                multipart_forms: vec![],
                data: Some(String::from("{\"test\": \"value\"}")),
            };
            let (resp, val) = req.make_request(&mut g_env, false, false).unwrap();
            let expected = r#"{
  "success": "true"
}"#;
            assert!(resp.contains(expected), "Expected {} in response, but response is {}", expected, resp);
            assert_eq!(val["success"], json!("true"), "Got incorrect value: {:?}", val);
        }
        {
            let req = Request {
                method: Method::Post,
                url: String::from("https://reqbin.com/echo/post/json"),
                headers: vec![String::from("{{.dne}}: application/json")],
                multipart_forms: vec![],
                data: Some(String::from("{\"test\": \"value\"}")),
            };
            let resp = req.make_request(&mut g_env, false, false);
            match resp {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "failed to get resource at .dne",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let req = Request {
                method: Method::Get,
                url: String::from("http://aunchoeu"),
                headers: vec![],
                multipart_forms: vec![],
                data: None,
            };
            let resp = req.make_request(&mut g_env, false, false);
            match resp {
                Ok(ret) => panic!("Expected error, but got Ok with value {:?}", ret),
                Err(e) => assert_eq!(
                    e.to_string(),
                    "curl: (6) Couldn't resolve host 'aunchoeu'\n",
                    "Got an incorrect error: \"{}\"",
                    e.to_string()
                ),
            };
        }
        {
            let req = Request {
                method: Method::Post,
                url: String::from("https://reqbin.com/echo/post/json"),
                headers: vec![String::from("{{.ct}}: {{.json}}")],
                multipart_forms: vec![],
                data: Some(String::from("{\"test\": \"value\"}")),
            };
            let (resp, val) = req.make_request(&mut g_env, true, false).unwrap();
            let expected = "curl -k --include https://reqbin.com/echo/post/json -X POST -H Content-Type: application/json -d {\"test\": \"value\"}";
            assert!(resp.contains(expected), "Expected {} in response, but response is {}", expected, resp);
            assert!(val.as_str().unwrap().is_empty(), "Expected val to be empty, got {}", val);
        }
        {
            let req = Request {
                method: Method::Post,
                url: String::from("https://reqbin.com/echo/post/json"),
                headers: vec![String::from("{{.ct}}: {{.json}}")],
                multipart_forms: vec![],
                data: Some(String::from("{\"test\": \"value\"}")),
            };
            let (resp, val) = req.make_request(&mut g_env, true, true).unwrap();
            let expected = "curl -k -v https://reqbin.com/echo/post/json -X POST -H Content-Type: application/json -d {\"test\": \"value\"}";
            assert!(resp.contains(expected), "Expected {} in response, but response is {}", expected, resp);
            assert!(val.as_str().unwrap().is_empty(), "Expected val to be empty, got {}", val);
        }
        {
            let req = Request {
                method: Method::Post,
                url: String::from("https://reqbin.com/echo/post/json"),
                headers: vec![String::from("{{.ct}}: {{.json}}")],
                multipart_forms: vec![],
                data: Some(String::from("{\"test\": \"value\"}")),
            };
            let (resp, val) = req.make_request(&mut g_env, false, true).unwrap();
            let expected1 = "> POST /echo/post/json";
            let expected2 = "< Content-Type: application/json";
            let expected3 = Regex::new(r"(?m)^<.* 200 OK$").unwrap();
            assert!(resp.contains(expected1), "Expected {} in response, but response is {}", expected1, resp);
            assert!(resp.contains(expected2), "Expected {} in response, but response is {}", expected2, resp);
            assert!(expected3.is_match(&resp), "Expected {} in response, but response is {}", "< HTTP/_ 200 OK", resp);
            assert_eq!(val["success"], json!("true"), "Got incorrect value: {:?}", val);
        }

        clear_env_file();
    }
}
