use lettre::{
    transport::smtp::{
        authentication::Credentials
    },
    Message,
    SmtpTransport,
    Transport
};

use super::CONF;
// use crate::orchestrator::ORC;

pub fn send_email(email: &Message) -> bool {
    let (mail_server, smtp_username, smtp_password, dev_mode) = {
        let conf = CONF.read();
        (
            conf.mail_server.clone(),
            conf.smtp_username.clone(),
            conf.smtp_password.clone(),
            conf.dev_mode
        )
    };
    if let Ok(transport) = SmtpTransport::starttls_relay(&mail_server) {
        let mailer = transport.credentials(
            Credentials::new(smtp_username, smtp_password)
        ).build();

        match mailer.send(&email) {
            Ok(response) => {
                return response.is_positive();
            },
            Err(e) => {
                if dev_mode {
                    println!("Could not send email: {:?}", e);
                }
                return false;
            },
        }
    }
    false
}
