//! Mail against a real Postgres and a local Mailpit: each message goes once, by SMTP to an address on the allowlist
//! and into the mailbox for anyone else, a message that may never be kept goes only to the allowlist, and a key a
//! send the server did not take is given back, while one whose outcome is unknown lapses after two minutes.

#[cfg(test)]
mod support;

use std::time::Duration;

use support::{
    Mailpit, PASSWORD, address, allow, as_owner, database, free_port, lapse, message, route,
};
use tetsu_kernel::{Delivery, MailError, Mailbox, Mailer};
use tetsu_mail::{Allowlist, Smtp, SmtpSettings, post_office};
use tetsu_postgres::{Settings, pool};

#[tokio::test]
async fn a_message_to_someone_off_the_allowlist_is_kept_once_for_the_mailbox() {
    let (pool, database) = database().await;
    let office = post_office(pool, None, Allowlist::default());
    let sent = message("ada@example.com", "invite:1");
    assert_eq!(office.send(&sent).await, Ok(Delivery::Captured));
    assert_eq!(office.send(&sent).await, Ok(Delivery::Repeated));
    assert_eq!(
        route(&database, "invite:1").await.as_deref(),
        Some("captured")
    );
    let mailbox = office.captured().await.expect("the mailbox");
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox[0].recipient, "ada@example.com");
    assert_eq!(mailbox[0].subject, "Your invite");
    assert_eq!(mailbox[0].text, "Open the link.");
    assert_eq!(mailbox[0].html, "<p>Open the link.</p>");
    assert!(mailbox[0].at.elapsed().expect("in the past") < Duration::from_mins(1));
}

#[tokio::test]
async fn the_mailbox_shows_the_newest_hundred_of_the_past_week_first() {
    let (pool, database) = database().await;
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
    as_owner(
        &database,
        "UPDATE mail.captured SET captured_at = now() - interval '8 days';
         UPDATE mail.sends SET claimed_at = now() - interval '31 days' WHERE message_key = 'k:000'",
    )
    .await;
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
    office
        .send(&message("ada@example.com", "fresh"))
        .await
        .expect("kept");
    let left: i64 = tetsu_postgres::connect(&database)
        .await
        .expect("connects")
        .query_one(
            "SELECT (SELECT count(*) FROM mail.captured) * 1000 + (SELECT count(*) FROM mail.sends)",
            &[],
        )
        .await
        .expect("counts")
        .try_get(0)
        .expect("a count");
    assert_eq!(
        left, 1102,
        "one kept, and every key but the one past thirty days"
    );
}

#[tokio::test]
async fn a_message_that_may_never_be_kept_goes_only_to_the_allowlist() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    let secret = || message("ada@example.com", "invite:9").never_kept();
    let kept_nowhere = post_office(pool.clone(), None, Allowlist::default());
    assert_eq!(kept_nowhere.send(&secret()).await, Ok(Delivery::Withheld));
    assert_eq!(kept_nowhere.captured().await.expect("the mailbox"), vec![]);
    assert_eq!(route(&database, "invite:9").await, None);
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(office.send(&secret()).await, Ok(Delivery::Sent));
    assert_eq!(mailpit.messages().await.len(), 1);
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
    let (pool, database) = database().await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    let sent = message("ada@example.com", "invite:2");
    assert_eq!(office.send(&sent).await, Ok(Delivery::Sent));
    assert_eq!(office.send(&sent).await, Ok(Delivery::Repeated));
    assert_eq!(route(&database, "invite:2").await.as_deref(), Some("sent"));
    let messages = mailpit.messages().await;
    assert_eq!(messages.len(), 1);
    let full = mailpit
        .message(messages[0]["ID"].as_str().expect("an ID"))
        .await;
    assert_eq!(full["To"][0]["Address"], "ada@example.com");
    assert_eq!(full["From"]["Address"], "tetsu@sandbox.example.com");
    assert_eq!(full["Subject"], "Your invite");
    assert_eq!(full["MessageID"], "696e766974653a32@sandbox.example.com");
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

/// A refused login is Tetsu's setup, not the message, so the key is given back and tried again at once.
#[tokio::test]
async fn a_refused_login_gives_the_key_back() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    let wrong =
        Smtp::trusting(&mailpit.settings("wrong"), &mailpit.certificate).expect("an adapter");
    let refused = post_office(pool.clone(), Some(wrong), allow("ada@example.com"));
    assert_eq!(
        refused.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(route(&database, "k").await, None);
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Sent)
    );
    assert_eq!(mailpit.messages().await.len(), 1);
}

/// A server that cannot be reached may have taken nothing, but a dropped connection looks the same, so the claim is
/// left to lapse rather than risk sending the message again at once.
#[tokio::test]
async fn a_server_that_cannot_be_reached_leaves_the_claim_to_lapse() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    let mut settings = mailpit.settings(PASSWORD);
    settings.port = free_port();
    let down = Smtp::trusting(&settings, &mailpit.certificate).expect("an adapter");
    let office = post_office(pool, Some(down), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(route(&database, "k").await.as_deref(), Some("sending"));
}

/// A recipient the server refuses for good is refused once, and settled, so no one tries again.
#[tokio::test]
async fn a_refused_recipient_is_refused_once() {
    let mailpit = Mailpit::start_allowing(Some("^ada@example\\.com$")).await;
    let (pool, database) = database().await;
    let office = post_office(
        pool,
        Some(mailpit.adapter()),
        allow("ada@example.com,lee@example.com"),
    );
    assert_eq!(
        office.send(&message("lee@example.com", "k")).await,
        Err(MailError::Refused)
    );
    assert_eq!(route(&database, "k").await.as_deref(), Some("refused"));
    assert_eq!(
        office.send(&message("lee@example.com", "k")).await,
        Ok(Delivery::Repeated)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

#[tokio::test]
async fn a_server_whose_certificate_is_not_trusted_is_never_sent_to() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    let untrusting = Smtp::new(&mailpit.settings(PASSWORD)).expect("an adapter");
    let office = post_office(pool, Some(untrusting), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(route(&database, "k").await.as_deref(), Some("sending"));
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

/// A trusted certificate for another name is refused too (`v5.0.0-12.3.2`).
#[tokio::test]
async fn a_certificate_for_another_name_is_never_sent_to() {
    let mailpit = Mailpit::start().await;
    let (pool, _database) = database().await;
    let mut settings = mailpit.settings(PASSWORD);
    settings.host = "127.0.0.1".to_string();
    let elsewhere = Smtp::trusting(&settings, &mailpit.certificate).expect("an adapter");
    let office = post_office(pool, Some(elsewhere), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

/// A server that takes the connection and never answers is given up on at the limit, not after each step's wait.
#[tokio::test]
async fn a_send_past_its_limit_is_unavailable() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let mut settings = mailpit.settings(PASSWORD);
    settings.port = silent.local_addr().expect("an address").port();
    settings.limit = Duration::from_millis(300);
    let slow = Smtp::trusting(&settings, &mailpit.certificate).expect("an adapter");
    let office = post_office(pool, Some(slow), allow("ada@example.com"));
    let started = std::time::Instant::now();
    assert_eq!(
        office.send(&message("ada@example.com", "notice:1")).await,
        Err(MailError::Unavailable)
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(
        route(&database, "notice:1").await.as_deref(),
        Some("sending"),
        "the server may have taken it, so the claim lapses"
    );
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
        limit: Duration::from_secs(5),
    };
    let pem = &mailpit.certificate;
    assert!(Smtp::new(&settings("smtp.example.com")).is_ok());
    assert_eq!(
        Smtp::new(&settings("user:secret@host")).err(),
        Some("the SMTP host is not a name TLS can check".to_string())
    );
    assert!(Smtp::trusting(&settings("smtp.example.com"), pem).is_ok());
    assert!(Smtp::trusting(&settings("smtp.example.com"), b"not a certificate").is_err());
    assert_eq!(
        Smtp::trusting(&settings("not a host"), pem).err(),
        Some(
            "the SMTP host is not a name TLS can check, or the PEM holds no certificate"
                .to_string()
        )
    );
}

#[tokio::test]
async fn a_message_refused_at_commit_keeps_nothing() {
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
        Err(MailError::Unavailable)
    );
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
    assert_eq!(route(&database, "k").await, None);
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
        Err(MailError::Unavailable)
    );
}

#[tokio::test]
async fn a_key_that_cannot_be_claimed_keeps_nothing() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE INSERT ON mail.sends FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(office.captured().await.expect("the mailbox"), vec![]);
}

/// A key another attempt is sending now is not taken, and the message is not kept twice; once that claim lapses,
/// it is.
#[tokio::test]
async fn a_key_being_sent_is_left_to_its_attempt_until_it_lapses() {
    let (pool, database) = database().await;
    as_owner(
        &database,
        "INSERT INTO mail.sends (message_key, route) VALUES ('k', 'sending')",
    )
    .await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    lapse(&database, "k").await;
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Captured)
    );
}

#[tokio::test]
async fn a_message_that_cannot_be_kept_leaves_its_key_free() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE INSERT ON mail.captured FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(route(&database, "k").await, None);
}

/// A key another attempt is sending now is not sent again.
#[tokio::test]
async fn a_key_being_sent_by_smtp_is_left_to_its_attempt() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    as_owner(
        &database,
        "INSERT INTO mail.sends (message_key, route) VALUES ('k', 'sending')",
    )
    .await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

#[tokio::test]
async fn a_failed_clean_up_keeps_nothing_and_leaves_the_key_free() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE DELETE ON mail.captured FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Err(MailError::Unavailable)
    );
    assert_eq!(route(&database, "k").await, None);
}

#[tokio::test]
async fn a_mailbox_that_cannot_be_read_is_unavailable() {
    let (pool, database) = database().await;
    as_owner(&database, "REVOKE SELECT ON mail.captured FROM tetsu_mail").await;
    let office = post_office(pool, None, Allowlist::default());
    assert_eq!(office.captured().await, Err(MailError::Unavailable));
}

#[tokio::test]
async fn a_kept_message_whose_column_reads_as_the_wrong_type_is_unavailable() {
    // A date still compares with the age filter, but reads as no time.
    for (column, retyped) in [
        ("recipient", "bytea USING convert_to(recipient, 'UTF8')"),
        ("subject", "bytea USING convert_to(subject, 'UTF8')"),
        ("text_body", "bytea USING convert_to(text_body, 'UTF8')"),
        ("html_body", "bytea USING convert_to(html_body, 'UTF8')"),
        ("captured_at", "date USING captured_at::date"),
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
                 ALTER TABLE mail.captured ALTER COLUMN {column} TYPE {retyped}"
            ),
        )
        .await;
        assert_eq!(
            office.captured().await,
            Err(MailError::Unavailable),
            "{column}"
        );
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
        Err(MailError::Unavailable)
    );
    assert_eq!(mailpit.messages().await, Vec::<serde_json::Value>::new());
}

/// A message that went counts as sent even when its key cannot be settled; its claim then lapses, which a reader
/// could see as a second copy with the same `Message-ID`.
#[tokio::test]
async fn a_sent_message_whose_key_cannot_be_settled_still_counts_as_sent() {
    let mailpit = Mailpit::start().await;
    let (pool, database) = database().await;
    as_owner(
        &database,
        "CREATE FUNCTION mail.refuse() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'refused to settle'; END $$;
         CREATE TRIGGER refuse BEFORE UPDATE ON mail.sends FOR EACH ROW
             WHEN (NEW.route = 'sent') EXECUTE FUNCTION mail.refuse();",
    )
    .await;
    let office = post_office(pool, Some(mailpit.adapter()), allow("ada@example.com"));
    assert_eq!(
        office.send(&message("ada@example.com", "k")).await,
        Ok(Delivery::Sent)
    );
    assert_eq!(route(&database, "k").await.as_deref(), Some("sending"));
    assert_eq!(mailpit.messages().await.len(), 1);
}
