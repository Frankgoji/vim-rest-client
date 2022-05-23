use vim_rest_client::{parse_input, ENV_FILE};

use std::fs;
use regex::Regex;

fn clear_env_file() {
    if let Err(_) = fs::remove_file(ENV_FILE) {
        println!("file doesn't exist")
    } else {
        println!("file deleted")
    }
}

#[test]
fn test_parse_input() {
    {
        let test_in = r#"###{
@baseUrl = "https://10.0.0.20:5443/api/v1"
###}"#;
        let test_out = r#"###{ executed (SUCCESS)
@baseUrl = "https://10.0.0.20:5443/api/v1"
########## RESULT
@baseUrl = "https://10.0.0.20:5443/api/v1"
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{
# defining some vars
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
###}"#;
        let test_out = r#"###{ executed (SUCCESS)
# defining some vars
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
########## RESULT
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ selection
@testUrl = "{{baseUrl}}/test"
@url1 = "{{urls[0]}}"
@objA= "{{obj.a}}"
###}"#;
        let test_out = r#"###{ selection executed (SUCCESS)
@testUrl = "{{baseUrl}}/test"
@url1 = "{{urls[0]}}"
@objA= "{{obj.a}}"
########## selection RESULT
@testUrl = "https://10.0.0.20:5443/api/v1/test"
@url1 = "https://10.0.0.20:5443/api/v1"
@objA = "test"
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ executed (SUCCESS)
@valid = "valid json"
@willErr = not valid json
@wontExecute = "won't execute even if valid"
###}"#;
        let test_out = r#"###{ executed (ERROR)
@valid = "valid json"
@willErr = not valid json
@wontExecute = "won't execute even if valid"
########## ERROR
@valid = "valid json"
expected ident at line 1 column 2
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ no selection
GET https://reqbin.com/echo/get/json
###}"#;
        let should_contain = r#"###{ no selection executed (SUCCESS)
GET https://reqbin.com/echo/get/json
########## no selection RESULT
"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert!(
            result.contains(should_contain),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
        assert!(
            result.contains("200 OK"),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
    }
    {
        let test_in = r#"###{ selection
# @name getJson
@baseUrl = "https://reqbin.com"
GET {{baseUrl}}/echo/get/json
###}"#;
        let should_contain = r#"###{ selection executed (SUCCESS)
# @name getJson
@baseUrl = "https://reqbin.com"
GET {{baseUrl}}/echo/get/json
########## selection RESULT
@baseUrl = "https://reqbin.com"
"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert!(
            result.contains(should_contain),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
        assert!(
            result.contains("200 OK"),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
    }
    {
        let test_in = r#"###{ test response executed (ERROR)
@test = "{{getJson.success}}"
###}"#;
        let test_out = r#"###{ test response executed (SUCCESS)
@test = "{{getJson.success}}"
########## test response RESULT
@test = "true"
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ test post executed (SUCCESS)
# @name postJson
POST {{baseUrl}}/echo/post/json
Content-Type: application/json

{
"test": "value",
"hello": "world"
}
###}"#;
        let should_contain = r#"###{ test post executed (SUCCESS)
# @name postJson
POST {{baseUrl}}/echo/post/json
Content-Type: application/json

{
"test": "value",
"hello": "world"
}
########## test post RESULT
"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert!(
            result.contains(should_contain),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
        assert!(
            result.contains("200 OK"),
            "Expected output should contain:\n{}\nResponse:\n{}",
            should_contain,
            result
        );
    }
    {
        let test_in = r#"###{ test response
@test = "{{postJson.success}}"
###}"#;
        let test_out = r#"###{ test response executed (SUCCESS)
@test = "{{postJson.success}}"
########## test response RESULT
@test = "true"
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"# This is a test

###{
# defining some vars
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
###}

# other vars
###{ set url
@test = "{{urls[1]}}/{{obj.b}}"
###}"#;
        let test_out = r#"# This is a test

###{ executed (SUCCESS)
# defining some vars
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
########## RESULT
@urls = ["https://10.0.0.20:5443/api/v1", "https://reqbin.com"]
@obj = {"a": "test", "b": "hello"}
###}

# other vars
###{ set url executed (SUCCESS)
@test = "{{urls[1]}}/{{obj.b}}"
########## set url RESULT
@test = "https://reqbin.com/hello"
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ outer
@test = "https://reqbin.com"
###{ inner
# @name innerReq
GET {{baseUrl}}/echo/get/json
###}
@res = "{{innerReq.success}}"
###}"#;
        let test_out = r#"(?s)###\{ outer executed \(SUCCESS\)
@test = "https://reqbin.com"
###\{ inner executed \(SUCCESS\)
# @name innerReq
GET \{\{baseUrl\}\}/echo/get/json
###\}
@res = "\{\{innerReq.success\}\}"
########## outer RESULT
@test = "https://reqbin.com"
### inner RESULT
.*
###
@res = "true"
###\}"#;
        let test_out_re = Regex::new(test_out).unwrap();
        let result = parse_input(&mut test_in.as_bytes());
        assert!(
            test_out_re.is_match(&result),
            "Result:\n{}",
            result
        );
    }
    {
        let test_in = r#"###{ outer
@test = "https://reqbin.com"
###{ inner success
@willSucceed = "{{test}}"
###}
###{ inner error
@willFail = "{{dne}}"
###}
@test2 = "{{willFail}}"
###}"#;
        let test_out = r#"###{ outer executed (ERROR)
@test = "https://reqbin.com"
###{ inner success executed (SUCCESS)
@willSucceed = "{{test}}"
###}
###{ inner error executed (ERROR)
@willFail = "{{dne}}"
###}
@test2 = "{{willFail}}"
########## outer ERROR
@test = "https://reqbin.com"
### inner success RESULT
@willSucceed = "https://reqbin.com"
###
### inner error ERROR
failed to get resource at dne
###
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ outer
@test = "https://reqbin.com"
###{ inner success
@willSucceed = "{{test}}"
###}
###{ inner error
@willFail = "{{dne}}"
###}
###}"#;
        let test_out = r#"###{ outer executed (ERROR)
@test = "https://reqbin.com"
###{ inner success executed (SUCCESS)
@willSucceed = "{{test}}"
###}
###{ inner error executed (ERROR)
@willFail = "{{dne}}"
###}
########## outer ERROR
@test = "https://reqbin.com"
### inner success RESULT
@willSucceed = "https://reqbin.com"
###
### inner error ERROR
failed to get resource at dne
###
###}"#;
        let result = parse_input(&mut test_in.as_bytes());
        assert_eq!(
            result,
            String::from(test_out),
            "Expected:\n{}\nGot:\n{}",
            test_out,
            result
        );
    }
    {
        let test_in = r#"###{ outer
@test = "https://reqbin.com"
# @name outerReq
GET {{baseUrl}}/echo/get/json
###{ inner
@res = "{{outerReq.success}}"
###}
###}"#;
        let test_out = r#"(?s)###\{ outer executed \(SUCCESS\)
@test = "https://reqbin.com"
# @name outerReq
GET \{\{baseUrl\}\}/echo/get/json
###\{ inner executed \(SUCCESS\)
@res = "\{\{outerReq.success\}\}"
###\}
########## outer RESULT
@test = "https://reqbin.com"
.*
### inner RESULT
@res = "true"
###
###\}"#;
        let test_out_re = Regex::new(test_out).unwrap();
        let result = parse_input(&mut test_in.as_bytes());
        assert!(
            test_out_re.is_match(&result),
            "Result:\n{}",
            result
        );
    }

    clear_env_file();
}
