use serde::Serialize;

// 分开传递命令查询与附带问题，避免把 -q 的语义变成普通位置参数。
#[derive(Serialize)]
pub struct Query {
    pub command_query: Option<String>,
    pub question: Option<String>,
}
