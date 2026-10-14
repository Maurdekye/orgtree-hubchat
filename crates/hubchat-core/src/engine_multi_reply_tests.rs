//! Real admission/store/wire boundary, with no network or engine task started.
use super::*;
use crate::questions::{Answer, ReplyMeta};

struct Quiet;
impl Host for Quiet {
    fn event(&self,_:Event) {}
    fn open_source(&self,_:&str)->std::io::Result<std::fs::File> { panic!("no file work expected") }
    fn download_dir(&self)->PathBuf { PathBuf::new() }
}
fn fixture()->Arc<Engine> {
    let store=Arc::new(Store::open_in_memory().unwrap());
    for (id,from) in [("q1","peer"),("q2","peer"),("q3","peer"),("foreign","other")] {
        let env:crate::hub::Envelope=serde_json::from_value(serde_json::json!({"id":id,"from":from,"to":"me","body":id,"kind":"question","received_at":"2026-10-14T16:00:00Z","meta":{"question":{"options":["Red","Green","Blue"],"multiple":true}}})).unwrap();
        store.insert_incoming("synthetic",&env,"2026-10-14T16:00:00Z").unwrap();
    }
    Engine::new(store,Identity::generate("multi").unwrap(),Profile::default(),Arc::new(Quiet))
}
fn message(id:&str,body:&str)->NewOutgoing { NewOutgoing{id:id.into(),peer:"peer".into(),body:body.into(),kind:None,reply_to:None,attachments:vec![]} }
fn mixed()->ReplyMeta { ReplyMeta{replies:vec!["q1".into(),"q2".into(),"q3".into()],answers:vec![Answer{question:"q2".into(),picked:vec!["Red".into(),"Green".into(),"Blue".into()]}]} }
#[tokio::test]
async fn mixed_text_and_picks_keep_all_targets_and_old_peer_context() {
    let e=fixture();e.send_with(message("mixed","Typed text"),mixed()).await.unwrap();
    let m=e.store.message("mixed").unwrap().unwrap();
    assert_eq!(m.body,"Typed text");assert_eq!(m.qa.replies,vec!["q1","q2","q3"]);
    let (wire,meta)=e.wire_body(&m,true).unwrap();
    assert_eq!(wire,"> q1\n> q2\nAnswer: Red, Green, Blue\n> q3\nTyped text");
    assert_eq!(meta.unwrap()["replies"],serde_json::json!(["q1","q2","q3"]));
    let (legacy,meta)=e.wire_body(&m,false).unwrap();assert_eq!(legacy,wire);assert!(meta.is_none());
}
#[tokio::test]
async fn empty_mixed_skips_only_text_targets() {
    let e=fixture();e.send_with(message("empty",""),mixed()).await.unwrap();
    let m=e.store.message("empty").unwrap().unwrap();let (wire,_)=e.wire_body(&m,true).unwrap();
    assert_eq!(wire,"> q1\nSkipped\n> q2\nAnswer: Red, Green, Blue\n> q3\nSkipped\n");
}
#[tokio::test]
async fn missing_and_foreign_conversation_targets_refuse_before_queue() {
    let e=fixture();e.send(message("own","Own source")).await.unwrap();
    for (id,target) in [("missing","unavailable"),("foreign-reply","foreign")] {
        let result=e.send_with(message(id,"Must refuse"),ReplyMeta{replies:vec!["q1".into(),target.into()],answers:vec![]}).await;
        assert!(result.is_err(),"{id}");assert!(e.store.message(id).unwrap().is_none(),"{id} was admitted");
    }
}

#[tokio::test]
async fn mixed_senders_in_one_conversation_keep_all_references_and_one_recipient() {
    let e=fixture();e.send(message("own","Own source")).await.unwrap();
    e.send_with(message("mixed-senders","One reply"),ReplyMeta{replies:vec!["q1".into(),"q2".into(),"own".into()],answers:vec![Answer{question:"q2".into(),picked:vec!["Red".into()]}]}).await.unwrap();
    let m=e.store.message("mixed-senders").unwrap().unwrap();
    assert_eq!(m.peer,"peer");assert_eq!(m.reply_to.as_deref(),Some("q1"));
    assert_eq!(m.qa.replies,vec!["q1","q2","own"]);
    let (wire,meta)=e.wire_body(&m,true).unwrap();
    assert_eq!(meta.unwrap()["replies"],serde_json::json!(["q1","q2","own"]));
    assert_eq!(wire,"> q1\n> q2\nAnswer: Red\n> Own source\nOne reply");
}
