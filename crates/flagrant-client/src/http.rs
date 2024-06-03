#[derive(Debug)]
pub enum HttpClient {
    Async(reqwest::Client, String),
    Blocking(reqwest::blocking::Client, String),
}
