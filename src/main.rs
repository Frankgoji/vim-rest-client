/// Vim REST Client helper script.
/// Parses output filtered from the .rest file by Vim.
///
/// Example input 1 (variables saved in some .env.json file):
/// ###{
/// @sshConfig = "~/.ssh/ssh.config"
/// @sshTo = "root@dut-1"
/// @baseUrl = "https://10.0.0.20:5443/api/v1"
/// ###}
///
/// Example output 1:
/// ###{ executed
/// @sshConfig = "~/.ssh/ssh.config"
/// @sshTo = "root@dut-1"
/// @baseUrl = "https://10.0.0.20:5443/api/v1"
/// ########## RESULT
/// @sshConfig = "~/.ssh/ssh.config"
/// @sshTo = "root@dut-1"
/// @baseUrl = "https://10.0.0.20:5443/api/v1"
/// ###}
///
/// Example input 2 (execute request, saves response in "resp"):
/// ###{ get reqbin
/// # @name resp
/// GET https://reqbin.com/echo/get/json
/// Content-Type: application/json
/// ###}
///
/// Example output 1:
/// ###{ get reqbin executed
/// # @name resp
/// GET https://reqbin.com/echo/get/json
/// Content-Type: application/json
/// ########## get reqbin RESULT
/// HTTP/1.1 200 OK
/// Date: Sat, 30 Apr 2022 09:07:16 GMT
/// Content-Type: application/json
/// Content-Length: 19
/// Connection: keep-alive
/// Access-Control-Allow-Origin: *
/// Last-Modified: Sat, 30 Apr 2022 07:21:29 GMT
/// Cache-Control: max-age=31536000
/// CF-Cache-Status: HIT
/// Age: 2078
/// Accept-Ranges: bytes
/// Expect-CT: max-age=604800, report-uri="https://report-uri.cloudflare.com/cdn-cgi/beacon/expect-ct"
/// Report-To: {"endpoints":[{"url":"https:\/\/a.nel.cloudflare.com\/report\/v3?s=vhr70s%2BTXe72%2FBtKwTgzpqo%2ByjJSjB9x
/// z2CSpC9BOX9pxgdSqNStYoGnMUzSloIfmlXlWBFN1wDZvuXL79UgWG6dmbfEKxwEQ8CuDGJ%2BuBDcBWMGUY77Ap8%2FXFcYHmrNFNv20OCLjacQ"}],
/// "group":"cf-nel","max_age":604800}
/// NEL: {"success_fraction":0,"report_to":"cf-nel","max_age":604800}
/// Server: cloudflare
/// CF-RAY: 703f20499a1f9879-SJC
/// alt-svc: h3=":443"; ma=86400, h3-29=":443"; ma=86400
///
/// {
///     "success": "true"
/// }
/// ###}
use std::env;
use std::io;

fn main() {
    if let Some(_) = env::args().find(|arg| &arg == &"-h" || &arg == &"--help") {
        usage();
        return;
    }
    // get filename from args (returns option)
    let mut args = env::args();
    let _binname = args.next();
    let filename = args.next();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut g_env = vim_rest_client::GlobalEnv::new(filename);
    println!("{}", g_env.parse_input(&mut handle, false));
}

fn usage() {
    println!("Usage of vim-rest-client:");
    println!("STDIN | vim-rest-client [-h/--help] [file]");
    println!();
    println!("\t--help/-h\t\tShow this usage message");
    println!("\tfile\t\t\tThe name to use as the env file (default .env.json)");
    println!();
    println!("Flags:");
    println!("# @name <name>\t\t\tSaves output from the fold result into the environment under the given name.");
    println!(
        "# @form <name>=<val>\t\tAdds multi-form data to the request. Equivalent to -F for curl."
    );
    println!(
        "# @debug\t\t\tDoes not execute fold but prints the curl command that would have executed."
    );
    println!("# @verbose\t\t\tEnables verbose logs.");
    println!("# @options <flags>\t\tAdds arguments to the argument list for curl.");
    println!();
    println!("Special Variables:");
    println!("sshTo\t\tHost to ssh to and run curl command from");
    println!("sshConfig\tSSH config file path");
    println!("sshKey\t\tSSH key file path");
    println!("sshPort\t\tPort of ssh host");
}
