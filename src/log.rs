use std::{fs::File, io::BufWriter};

use time::{OffsetDateTime, macros::format_description};

pub fn create_sse_log() -> anyhow::Result<BufWriter<File>> {
    let now = OffsetDateTime::now_utc();
    let format = format_description!("[year]-[month]-[day]-[hour]-[minute]-[second]");
    let filename = format!("/tmp/{}-sse.log", now.format(&format)?);
    Ok(BufWriter::new(File::create(filename)?))
}
