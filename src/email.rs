use lettre::{
    transport::smtp::{
        authentication::Credentials
    },
    Message,
    SmtpTransport,
    Transport
};

use super::CONF;
use crate::orchestrator::{ORC, Orchestrator};

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
}


