//! Mail against a real Postgres and a local Mailpit: each message goes once, by SMTP to an address on the allowlist
//! and into the mailbox for anyone else, and a failed send leaves its key free to try again.

#[cfg(test)]
mod support;

use std::time::Duration;

use support::{Mailpit, PASSWORD, address, allow, as_owner, database, free_port, message};
use tetsu_kernel::{Delivery, MailError, Mailbox, Mailer, Message};
use tetsu_mail::{Allowlist, Smtp, SmtpSettings, post_office};
use tetsu_postgres::{Settings, pool};

#[tokio::test]
async fn a_message_to_someone_off_the_allowlist_is_kept_once_for_the_mailbox() {
    let (pool, _database) = database().await;
    let office = post_office(pool, None, Allowlist::default());
    let sent = message("ada@example.com", "invite:1");
    assert_eq!(office.send(&sent).await, Ok(Delivery::Captured));
    assert_eq!(office.send(&sent).await, Ok(Delivery::Repeated));
    let mailbox = office.captured().await.expect("the mailbox");
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox[0].recipient, "ada@example.com");
    assert_eq!(mailbox[0].subject, "Your invite");
    assert_eq!(mailbox[0].text, "Open the link.");
    assert_eq!(mailbox[0].html, "<p>Open the link.</p>");
    assert!(mailbox[0].at.elapsed().expect("in the past") < Duration::from_mins(1));
}

#[tokio::test]
async fn the_mailbox_shows_the_newest_hundred_first() {
    let (pool, _database) = database().await;
    let office = post_office(pool, None, Allowlist::default());
    for n in 0..102 {
        office
            .send(&message("ada@example.com", &format!("k:{n:03}")))
            .await
            .expect("kept");
    }
    let mailbox = office.captured().await.expect("the mailbox");
    assert_eq!(mailbox.len(), 100);
    assert!(mailbox.windows(2).all(|pair| pair[0].at >= pair[1].at));
}

#[tokio::test]
async fn a_message_the_store_refuses_is_refused_and_leaves_its_key_free() {
    let (pool, _database) = database().await;
    let office = post_office(pool, None, Allowlist::default());
    let long = Message::new(
        address("ada@example.com"),
        &"s".repeat(999),
        "t",
        "h",
        "long",
    )
    .expect("a message");
    assert_eq!(office.send(&long).await, Err(MailError::Refused));
    let fixed =
        Message::new(address("ada@example.com"), "fine", "t", "h", "long").expect("a message");
    assert_eq!(office.send(&fixed).await, Ok(Delivery::Captured));
}

#[tokio::test]
async fn an_unreachable_store_is_unavailable() {
    let url = format!("postgres://tetsu@127.0.0.1:{}/nothing", free_port());
    let settings = Settings::from_values(&url, Some("disable"));
    let office = post_office(pool(&settings).expect("pool"), None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(office.captured().await, Err(MailError::Unavailable));
}

#[tokio::test]
async fn a_message_to_the_allowlist_goes_out_once_by_smtp_over_tls() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    let sent = message("ada@example.com", "invite:2");
    assert_eq!(office.send(&sent).await, Ok(Delivery::Sent));
    assert_eq!(office.send(&sent).await, Ok(Delivery::Repeated));
    let messages = mailpit.messages().await;
    assert_eq!(messages.len(), 1);
    let full = mailpit
        .message(messages[0]["ID"].as_str().expect("an ID"))
        .await;
    assert_eq!(full["To"][0]["Address"], "ada@example.com");
    assert_eq!(full["From"]["Address"], "tetsu@sandbox.example.com");
    assert_eq!(full["Subject"], "Your invite");
    assert_eq!(full["Text"].as_str().map(str::trim), Some("Open the link."));
    assert_eq!(
        full["HTML"].as_str().map(str::trim),
        Some("<p>Open the link.</p>")
    );
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
}

#[tokio::test]
async fn with_smtp_set_anyone_off_the_allowlist_is_still_kept() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("eve@example.com", "k")).await,
        Ok(Delivery::Captured)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

#[tokio::test]
async fn a_refused_login_is_refused_and_leaves_the_key_free() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let wrong =
        Smtp::trusting(&mailpit.settings("wrong"), &mailpit.certificate).expect("an adapter");
    let refused = post_office(pool.clone(), Some(wrong), allow("ada@example.com"));
    assert_eq!(
        refused.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Sent)
    );
    assert_eq!(mailpit.messages().await.len(), 1);
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_unavailable_and_leaves_the_key_free() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let mut settings = mailpit.settings(PASSWORD);
    settings.port = free_port();
    let down = Smtp::trusting(&settings, &mailpit.certificate).expect("an adapter");
    let office = post_office(pool.clone(), Some(down), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Sent)
    );
}

#[tokio::test]
async fn a_server_whose_certificate_is_not_trusted_is_never_sent_to() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let untrusting = Smtp::new(&mailpit.settings(PASSWORD)).expect("an adapter");
    let office = post_office(pool, Some(untrusting), allow("ada@example.com"));
    assert!(office.send(&message("ada@example.com", "k")).await.is_err());
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

#[tokio::test]
async fn an_adapter_needs_a_host_tls_can_check_and_a_certificate_it_can_read() {
    let mailpit = Mailpit::start().await;
    let settings = |host: &str| SmtpSettings {
        host: host.to_string(),
        port: 465,
        username: "u".to_string(),
        password: "p".to_string(),
        from: address("tetsu@example.com"),
    };
    let pem = &mailpit.certificate;
    assert!(Smtp::new(&settings("smtp.example.com")).is_ok());
    assert_eq!(
        Smtp::new(&settings("not a host")).err(),
        Some("the SMTP host \"not a host\" is not a name TLS can check".to_string())
    );
    assert!(Smtp::trusting(&settings("smtp.example.com"), pem).is_ok());
    assert!(Smtp::trusting(&settings("smtp.example.com"), b"not a certificate").is_err());
    assert_eq!(
        Smtp::trusting(&settings("not a host"), pem).err(),
        Some(
            "the SMTP host \"not a host\" is not a name TLS can check, or the PEM holds no certificate"
                .to_string()
        )
    );
}

#[tokio::test]
async fn a_message_refused_at_commit_is_refused_and_leaves_nothing() {
    let (pool, database) = database().await;
    as_owner(
        &database,
        "CREATE FUNCTION mail.refuse() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'refused at commit'; END $$;
             CREATE CONSTRAINT TRIGGER refuse AFTER INSERT ON mail.captured
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION mail.refuse();",
    )
    .await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
}

#[test]
fn mail_registers_its_own_schema() {
    use tetsu_kernel::Module;
    assert_eq!(tetsu_mail::Mail.name(), "mail");
    assert_eq!(
        tetsu_mail::Mail.storage().map(|storage| storage.schema),
        Some("mail")
    );
}

#[tokio::test]
async fn a_transaction_that_cannot_start_keeps_nothing() {
    let (pool, database) = database().await;
    as_owner(
        &database,
        "REVOKE EXECUTE ON FUNCTION pg_catalog.set_config(text, text, boolean) FROM PUBLIC",
    )
    .await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
}

#[tokio::test]
async fn a_key_that_cannot_be_claimed_keeps_nothing() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE INSERT ON mail.sends FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
}

#[tokio::test]
async fn a_failed_clean_up_keeps_nothing_and_leaves_the_key_free() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE DELETE ON mail.captured FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
    as_owner(&database, "GRANT DELETE ON mail.captured TO tetsu_mail").await;
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Captured)
    );
}

#[tokio::test]
async fn a_mailbox_that_cannot_be_read_is_refused() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE SELECT ON mail.captured FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(office.captured().await, Err(MailError::Refused));
}

#[tokio::test]
async fn a_kept_message_whose_column_reads_as_the_wrong_type_is_refused() {
    for column in [
        "recipient",
        "subject",
        "text_body",
        "html_body",
        "captured_at",
    ] {
        let (pool, database) = database().await;
        let office = post_office(pool, None, Allowlist::default());
        office
            .send(&message("ada@example.com", "k"))
            .await
            .expect("kept");
        as_owner(
            &database,
            &format!(
                "ALTER TABLE mail.captured DROP CONSTRAINT IF EXISTS captured_{column}_check;
                 ALTER TABLE mail.captured ALTER COLUMN {column} DROP DEFAULT;
                 ALTER TABLE mail.captured ALTER COLUMN {column} TYPE bytea USING convert_to({column}::text, 'UTF8')"
            ),
        )
        .await;
        assert_eq!(office.captured().await, Err(MailError::Refused), "{column}");
    }
}

#[tokio::test]
async fn a_sending_key_that_cannot_be_claimed_sends_nothing() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE INSERT ON mail.sends FROM tetsu_mail").await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

#[tokio::test]
async fn a_failed_send_whose_key_cannot_be_given_back_is_refused() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE DELETE ON mail.sends FROM tetsu_mail").await;
    let mut settings = mailpit.settings(PASSWORD);
    settings.port = free_port();
    let down = Smtp::trusting(&settings, &mailpit.certificate).expect("an adapter");
    let office = post_office(pool, Some(down), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Refused)
    );
}
