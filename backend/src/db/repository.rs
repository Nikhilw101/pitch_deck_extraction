use mongodb::Database;

pub struct Repository {
    #[allow(dead_code)]
    db: Database,
}

impl Repository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
