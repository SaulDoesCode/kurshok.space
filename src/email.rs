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
    let conf = CONF.read();
    if let Ok(transport) = SmtpTransport::starttls_relay(&conf.mail_server) {
        let creds = Credentials::new(
            conf.smtp_username.clone(),
            conf.smtp_password.clone()
        );

        let mailer = transport
            .credentials(creds)
            .build();

        match mailer.send(&email) {
            Ok(response) => {
                return response.is_positive();
            },
            Err(e) => {
                if conf.dev_mode {
                    println!("Could not send email: {:?}", e);
                }
                return false;
            },
        }
    }
    false
}
