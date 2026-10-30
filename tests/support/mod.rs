use mockito::Request;
use serde_json::Value;

pub fn request_query(request: &Request) -> Value {
    let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap()
}
