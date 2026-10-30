use mockito::Request;
use serde_json::Value;

pub fn request_query(request: &Request) -> Value {
    let body: Value = serde_json::from_slice(request.body().unwrap()).unwrap();
    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap()
}

// A valid protocol fixture; expected CLI output remains independently asserted by each test.
pub fn openai_stream(text: &str) -> String {
    format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({"choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
    )
}
