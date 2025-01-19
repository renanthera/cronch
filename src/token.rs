const TOKENPATH: &str = "token.tk";

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Token {
    access_token: String,
    token_type: String,
    expires_in: i32,
    expires_at: f64,
}

impl Token {
    fn fmt(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }

    pub fn load() -> Result<String, ::std::io::Error> {
        use std::{fs::File, io::prelude::*};

        let mut file = File::open(TOKENPATH)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        match serde_json::from_str::<Token>(&contents) {
            Ok(json) => Ok(json.fmt()),
            Err(_) => todo!(),
        }
    }
}
