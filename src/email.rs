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
use crate::{
    orchestrator::{ORC, Orchestrator}
};

// TODO: Send emails in non-blocking tokio tasks

lazy_static!{
    static ref EMAIL_CONF: EmailConf = {
        let conf = CONF.read();
        EmailConf{
            mail_server: conf.mail_server.clone(),
            smtp_username: conf.smtp_username.clone(),
            smtp_password: conf.smtp_password.clone(),
        }
    };
}

struct EmailConf {
    mail_server: String,
    smtp_username: String,
    smtp_password: String,
}

impl Orchestrator {
    pub fn send_email(&self, email: Message) -> bool {
        if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
            let mailer = transport.credentials(Credentials::new(
                EMAIL_CONF.smtp_username.clone(),
                EMAIL_CONF.smtp_password.clone()
            )).build();
    
            match mailer.send(&email) {
                Ok(response) => {
                    let positive = response.is_positive();
                    return positive;
                },
                Err(e) => {
                    if self.dev_mode {
                        println!("Could not send email: {:?}", e);
                    }
                    return false;
                },
            }
        }
        false
    }

    pub fn send_email_indirect(&self, email: Message) {
        tokio::spawn(async move {
            if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
                let mailer = transport.credentials(Credentials::new(
                    EMAIL_CONF.smtp_username.clone(),
                    EMAIL_CONF.smtp_password.clone()
                )).build();

                match mailer.send(&email) {
                    Ok(_response) => {},
                    Err(e) => {
                        if ORC.dev_mode {
                            println!("Could not send email: {:?}", e);
                        }
                    },
                }
            }
        });
    }

    pub fn send_email_with_status_identifier(&self, sid: Vec<u8>, email: Message) {
        tokio::spawn(async move {
            if let Ok(transport) = SmtpTransport::starttls_relay(&EMAIL_CONF.mail_server) {
                let mailer = transport.credentials(Credentials::new(
                    EMAIL_CONF.smtp_username.clone(),
                    EMAIL_CONF.smtp_password.clone()
                )).build();

                if let Ok(Some(_)) = ORC.email_statuses.get(&sid) {
                    return // TODO: handle this with websocket notifications
                }
                if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Sending.try_to_vec().unwrap()) {
                    return
                }
                ORC.expire_key(60 * 6, "email_statuses".to_string(), &sid);

                match mailer.send(&email) {
                    Ok(response) => {
                        if response.is_positive() {
                            if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Sent.try_to_vec().unwrap()) {}
                            ORC.unexpire_key("email_statuses".to_string(), &sid);
                            ORC.expire_key(60 * 6, "email_statuses".to_string(), &sid);
                        } else {
                            let reason = Some(response.message.join("\n"));
                            if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Failed(reason).try_to_vec().unwrap()) {}
                            ORC.unexpire_key("email_statuses".to_string(), &sid);
                            ORC.expire_key(60 * 6, "email_statuses".to_string(), &sid);
                        }
                    },
                    Err(e) => {
                        if ORC.dev_mode {
                            println!("Could not send email: {:?}", e);
                        }
                        let reason = Some(format!("failed to send email: {}", e));
                        if let Err(_) = ORC.email_statuses.insert(&sid, EmailStatus::Failed(reason).try_to_vec().unwrap()) {}
                        ORC.unexpire_key("email_statuses".to_string(), &sid);
                        ORC.expire_key(60 * 6, "email_statuses".to_string(), &sid);
                    },
                }
            }
        });
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
pub enum EmailStatus{
    Sending,
    Sent,
    Failed(Option<String>)
}