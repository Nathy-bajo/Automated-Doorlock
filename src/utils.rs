use crate::auth::SALT;
use argon2::Config;

pub struct Hasher {
    salt: [u8; 32],
    config: Config<'static>,
}

impl Hasher {
    pub fn new() -> Self {
        let salt = SALT;
        let config = Config::default();
        Self { salt, config }
    }

    pub fn hash(&self, password: &str) -> Result<String, argon2::Error> {
        argon2::hash_encoded(password.as_bytes(), &self.salt, &self.config)
    }

    pub fn verify(&self, password: &str, hashed: &str) -> Result<bool, argon2::Error> {
        argon2::verify_encoded(hashed, password.as_bytes())
    }
}