use mongodb::{options::ClientOptions, Client};

pub async fn init_db(db_url: &str) -> Result<Client, mongodb::error::Error> {
    let mut client_options = ClientOptions::parse(db_url).await?;
    client_options.app_name = Some("PitchDeckExtractor".to_string());
    Client::with_options(client_options)
}
