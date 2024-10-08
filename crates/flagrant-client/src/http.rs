type Host = String;

#[derive(Debug)]
pub enum Auth {
    Token(String),
    None,
}

#[derive(Debug)]
pub enum HttpClient {
    Async(reqwest::Client, Host, Auth),
    Blocking(reqwest::blocking::Client, Host, Auth),
}

