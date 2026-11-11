use super::*;
use crate::pictures::{PictureRecord, MAX_WIRE_BYTES};

#[cfg(not(target_arch = "wasm32"))]
fn http()-> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(||reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(10)).build().expect("picture HTTP"))
}
/// A browser's client takes no settings: each request carries the limit (`WHOLE`), and
/// `redirected` refuses what the native client never follows.
#[cfg(target_arch = "wasm32")]
fn http()-> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}
/// The whole request's time limit (natively the client's own, the same).
const WHOLE: Duration = Duration::from_secs(10);
/// The answer came from somewhere else than the address asked: a browser follows a
/// redirect by itself, and a picture is taken only from the hub that was asked.
#[cfg(target_arch = "wasm32")]
fn redirected(response:&reqwest::Response, asked:&str)->bool {
    !Url::parse(asked).is_ok_and(|a|a.origin()==response.url().origin() && a.path()==response.url().path())
}
#[cfg(not(target_arch = "wasm32"))]
fn redirected(_response:&reqwest::Response, _asked:&str)->bool { false }
fn refused()->Error { Error::Hub{status:502,detail:"Profile picture request failed".into()} }
async fn picture_answer(response:reqwest::Response, peer:bool)->Result<Option<PictureRecord>> {
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
    #[derive(Deserialize)] struct Answer { picture: Option<PictureRecord>, #[serde(default)] picture_owner:Option<String> }
    let mut answer:Answer=serde_json::from_slice(&body).map_err(|_|Error::Invalid("Invalid picture response".into()))?;
    if peer && !answer.picture_owner.as_deref().is_some_and(crate::pictures::valid_owner) { return Err(Error::Invalid("Missing picture identity".into())); }
    if let Some(p)=&mut answer.picture { p.meta.picture_owner=if peer {answer.picture_owner} else {None}; }
    if let Some(p)=&answer.picture { p.decoded()?; }
    Ok(answer.picture)
}
impl HubClient {
    pub async fn picture(&self, me:&Identity, peer:&str)->Result<Option<PictureRecord>> {
        self.session_work(async {
        let asked=self.addr.join("/api/profile-picture");
        let response=http().get(&asked).query(&[("slug",peer)])
            .header("X-Org-Auth",me.auth_header()).timeout(WHOLE).send().await?;
        if redirected(&response,&asked) { return Err(refused()); }
        picture_answer(response,peer!=me.address()).await
        }).await
    }
    pub async fn set_picture(&self, me:&Identity, record:&PictureRecord)->Result<PictureRecord> {
        self.session_work(async {
        record.decoded()?;
        let asked=self.addr.join("/api/profile-picture");
        let response=http().post(&asked).header("X-Org-Auth",me.auth_header())
            .json(&serde_json::json!({"slug":me.address(),"picture":record})).timeout(WHOLE).send().await?;
        if redirected(&response,&asked) { return Err(refused()); }
        picture_answer(response,false).await?.ok_or_else(||Error::Invalid("Missing picture acknowledgement".into()))
        }).await
    }
}
