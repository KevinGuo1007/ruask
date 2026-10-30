use std::error::Error;
use std::io::BufRead;

pub(super) struct Event {
    pub name: String,
    pub data: String,
}

// Frame complete SSE events before either protocol interprets their JSON payload.
pub(super) fn read_event(reader: &mut impl BufRead) -> Result<Event, Box<dyn Error>> {
    let mut name = String::new();
    let mut data = Vec::new();
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .map_err(|error| format!("模型响应不完整：读取流失败（{error}）"))?
            == 0
        {
            return Err("模型响应不完整：流意外结束".into());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            let payload = data.join("\n");
            if !payload.trim().is_empty() {
                return Ok(Event {
                    name,
                    data: payload,
                });
            }
            data.clear();
            name.clear();
        } else if let Some((field, value)) = line.split_once(':') {
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "event" => name = value.to_owned(),
                "data" => data.push(value.to_owned()),
                _ => {}
            }
        }
    }
}
