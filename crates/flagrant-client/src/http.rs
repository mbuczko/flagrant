type Host = String;

#[derive(Debug)]
pub enum HttpClient {
    Async(reqwest::Client, Host),
    Blocking(reqwest::blocking::Client, Host),
}
