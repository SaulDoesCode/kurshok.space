use borsh::{BorshDeserialize, BorshSerialize};
use lettre::{
    transport::smtp::{
        authentication::Credentials
    },
    Message,
    SmtpTransport,
    Transport
};

use super::CONF;
use crate::{expirable_data::ExpirableData, orchestrator::{ORC}};

use std::lazy::SyncLazy;

static EMAIL_CONF: SyncLazy<EmailConf> = SyncLazy::new(|| {
    let conf = CONF.read();
    EmailConf{
        mail_server: conf.mail_server.clone(),
        smtp_username: conf.smtp_username.clone(),
        smtp_password: conf.smtp_password.clone(),
    }
});

struct EmailConf {
    mail_server: String,
    smtp_username: String,
    smtp_password: String,
}
/*
pub fn send_email(email: Message) -> bool {
    if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
        let mailer = transport
            .credentials(Credentials::new(
                EMAIL_CONF.smtp_username.clone(),
                EMAIL_CONF.smtp_password.clone(),
            ))
            .build();

        match mailer.send(&email) {
            Ok(response) => {
                let positive = response.is_positive();
                return positive;
            }
            Err(e) => {
                if ORC.dev_mode {
                    println!("Could not send email: {:?}", e);
                }
                return false;
            }
        }
    }
    false
}

pub fn send_email_indirect(email: Message) {
    tokio::task::spawn_blocking(move || {
        if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
            let mailer = transport
                .credentials(Credentials::new(
                    EMAIL_CONF.smtp_username.clone(),
                    EMAIL_CONF.smtp_password.clone(),
                ))
                .build();

            match mailer.send(&email) {
                Ok(_response) => {}
                Err(e) => {
                    if ORC.dev_mode {
                        println!("Could not send email: {:?}", e);
                    }
                }
            }
        }
    });
}
*/
pub fn send_email_with_status_identifier(sid: Vec<u8>, email: Message) {
    if let Ok(Some(_)) = ORC.email_statuses.get(&sid) {
        return; // TODO: handle this with websocket notifications
    }

    tokio::task::spawn_blocking(move || {
        if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
            let mailer = transport
                .credentials(Credentials::new(
                    EMAIL_CONF.smtp_username.clone(),
                    EMAIL_CONF.smtp_password.clone(),
                ))
                .build();

            if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Sending.try_to_vec().unwrap()) {
                return;
            }

            let mut unexpire_key = vec![];
            unexpire_key.extend_from_slice(b"sid:");
            unexpire_key.extend_from_slice(&sid);

            let exp_data = ExpirableData::Single {
                tree: "email_statuses".to_string(),
                key: sid.clone(),
            };

            ORC.expire_data(60 * 6, exp_data, Some(&unexpire_key));

            match mailer.send(&email) {
                Ok(response) => {
                    if response.is_positive() {
                        if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Sent.try_to_vec().unwrap()) {}
                    } else {
                        let reason = Some(response.message.join("\n"));
                        if let Err(_) = ORC
                            .email_statuses
                            .insert(&sid, EmailStatus::Failed(reason).try_to_vec().unwrap())
                        {
                        }
                    }
                }
                Err(e) => {
                    if ORC.dev_mode {
                        println!("Could not send email: {:?}", e);
                    }
                    let reason = Some(format!("failed to send email: {}", e));
                    if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Failed(reason).try_to_vec().unwrap()) {}
                }
            }
        } else {
            if ORC.email_statuses.insert(
                &sid,
                EmailStatus::Failed(Some(
                    "could not start smtp connection".to_string()
                )).try_to_vec().unwrap()
            ).is_ok() {
                let mut unexpire_key = vec![];
                unexpire_key.extend_from_slice(b"sid:");
                unexpire_key.extend_from_slice(&sid);

                let exp_data = ExpirableData::Single {
                    tree: "email_statuses".to_string(),
                    key: sid.clone(),
                };

                ORC.expire_data(60 * 6, exp_data, Some(&unexpire_key));
            }
        }
    });
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum EmailStatus{
    Sending,
    Sent,
    Failed(Option<String>)
}