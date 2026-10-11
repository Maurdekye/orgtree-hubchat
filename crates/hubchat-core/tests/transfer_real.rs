//! Requires a disposable real hub; absence is an error, never a silent pass.
use hubchat_core::{
    download::{DownloadCheckpoint, DownloadSpec},
    hub::{CancelFlag, Outgoing, Profile},
    HubAddress, HubClient, Identity,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
#[tokio::test]
#[ignore = "requires a disposable real hub"]
async fn sha_upload_lost_completion_delivery_and_range_download_on_real_hub() {
    let url = std::env::var("HUBCHAT_V2_HUB").expect("set HUBCHAT_V2_HUB to a disposable hub");
    let c = HubClient::new(HubAddress::parse(&url).unwrap());
    let sender = Identity::generate("real-transfer").unwrap();
    let recipient = Identity::generate("real-recipient").unwrap();
    let profile = Profile {
        kind: "person".into(),
        ..Default::default()
    };
    c.register(&sender, &profile).await.unwrap();
    c.register(&recipient, &profile).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source");
    let bytes: Vec<u8> = (0..9 * 1024 * 1024usize).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &bytes).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    let upload = c
        .open_upload_checked(&sender, "proof.bin", bytes.len() as u64, Some(&digest))
        .await
        .unwrap();
    let done = c
        .resume_upload(
            &sender,
            &upload.id,
            tokio::fs::File::open(&src).await.unwrap(),
            None,
            CancelFlag::default(),
        )
        .await
        .unwrap();
    assert!(done.complete);
    let replay = c.upload_state(&sender, &upload.id).await.unwrap();
    assert!(replay.complete && replay.offset == bytes.len() as u64);
    let mut message = Outgoing::new(&recipient.address(), "only after the file is ready");
    message.attachments.push(upload.id.clone());
    let sent = c.send(&sender, &message).await.unwrap();
    let got = c.poll(&recipient, 0).await.unwrap();
    assert!(got
        .messages
        .iter()
        .any(|m| m.id == sent.id && m.attachments[0].id == upload.id));
    let response = reqwest::Client::new()
        .get(format!("{url}/api/attachments/{}", upload.id))
        .header(
            "X-Org-Auth",
            format!("{}:{}", recipient.address(), recipient.secret()),
        )
        .header("Range", "bytes=0-0")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 206);
    let validator = response
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[..1]);
    let partial = dir.path().join("private.part");
    std::fs::write(&partial, &bytes[..8 * 1024 * 1024]).unwrap();
    let cp = DownloadCheckpoint {
        confirmed: 8 * 1024 * 1024,
        validator: Some(validator),
        updated_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    };
    let saved = Arc::new(Mutex::new(cp.clone()));
    let saved2 = saved.clone();
    c.download_resumable(
        &recipient,
        DownloadSpec {
            attachment_id: &upload.id,
            partial: &partial,
            total: bytes.len() as u64,
            checkpoint: cp,
        },
        Arc::new(move |cp| {
            *saved2.lock().unwrap() = cp.clone();
            Ok(())
        }),
        None,
        CancelFlag::default(),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&partial).unwrap(), bytes);
    assert_eq!(saved.lock().unwrap().confirmed, bytes.len() as u64);
    let wrong = c
        .open_upload_checked(
            &sender,
            "wrong.bin",
            bytes.len() as u64,
            Some(&"0".repeat(64)),
        )
        .await
        .unwrap();
    let error = c
        .resume_upload(
            &sender,
            &wrong.id,
            tokio::fs::File::open(src).await.unwrap(),
            None,
            CancelFlag::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(422));
    assert_eq!(c.upload_state(&sender, &wrong.id).await.unwrap().offset, 0);
}
