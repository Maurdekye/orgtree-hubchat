use super::*;
use crate::pictures::{PictureRecord, MAX_WIRE_BYTES};

fn http()-> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(||reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(10)).build().expect("picture HTTP"))
}
async fn picture_answer(response:reqwest::Response)->Result<Option<PictureRecord>> {
    let status=response.status();
    // Never read an unbounded error body or include image bytes in diagnostics.
    if !status.is_success() { return Err(Error::Hub{status:status.as_u16(),detail:"Profile picture request failed".into()}); }
    if response.content_length().is_some_and(|n|n>MAX_WIRE_BYTES as u64) { return Err(Error::Invalid("Picture response exceeds limit".into())); }
    let mut body=Vec::new();let mut stream=response.bytes_stream();
    while let Some(chunk)=stream.next().await {
        let chunk=chunk?;
        if body.len()+chunk.len()>MAX_WIRE_BYTES { return Err(Error::Invalid("Picture response exceeds limit".into())); }
        body.extend_from_slice(&chunk);
    }
    #[derive(Deserialize)] struct Answer { picture: Option<PictureRecord> }
    let answer:Answer=serde_json::from_slice(&body).map_err(|_|Error::Invalid("Invalid picture response".into()))?;
    if let Some(p)=&answer.picture { p.decoded()?; }
    Ok(answer.picture)
}
impl HubClient {
    pub async fn picture(&self, me:&Identity, peer:&str)->Result<Option<PictureRecord>> {
        self.session_work(async {
        let response=http().get(self.addr.join("/api/profile-picture")).query(&[("slug",peer)])
            .header("X-Org-Auth",me.auth_header()).send().await?;
        picture_answer(response).await
        }).await
    }
    pub async fn set_picture(&self, me:&Identity, record:&PictureRecord)->Result<PictureRecord> {
        self.session_work(async {
        record.decoded()?;
        let response=http().post(self.addr.join("/api/profile-picture")).header("X-Org-Auth",me.auth_header())
            .json(&serde_json::json!({"slug":me.address(),"picture":record})).send().await?;
        picture_answer(response).await?.ok_or_else(||Error::Invalid("Missing picture acknowledgement".into()))
        }).await
    }
}
