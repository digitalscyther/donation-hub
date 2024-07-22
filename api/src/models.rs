use serde::{Serialize, Deserialize};
use sqlx::{Error, PgPool};
use sqlx::postgres::PgQueryResult;
use sqlx::types::{Uuid, Decimal};

#[derive(Serialize, Deserialize)]
pub struct JsonDonation {
    pub id: Option<String>,
    pub amount: Decimal,
    pub title: String,
    pub description: Option<String>,
    pub webhook: Option<String>,
    pub wallet_id: Option<String>,
}

pub struct Donation {
    pub id: Uuid,
    pub amount: Decimal,
    pub title: String,
    pub description: Option<String>,
    pub webhook: Option<String>,
    pub wallet_id: Option<Uuid>,
    pub user_id: Option<Uuid>
}

impl Into<Donation> for JsonDonation {
    fn into(self) -> Donation {
        Donation {
            id: self.id.map_or_else(Uuid::new_v4, |id_str| Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4())),
            amount: self.amount,
            title: self.title,
            description: self.description,
            webhook: self.webhook,
            wallet_id: None,
            user_id: None
        }
    }
}

impl Into<JsonDonation> for Donation {
    fn into(self) -> JsonDonation {
        JsonDonation {
            id: Some(self.id.to_string()),
            amount: self.amount,
            title: self.title,
            description: self.description,
            webhook: self.webhook,
            wallet_id: self.wallet_id.map_or_else(|| None, |wid| Some(wid.to_string())),
        }
    }
}

impl Donation {
    pub async fn create(self, user_id: Uuid, db: &PgPool) -> Result<Donation, Error> {
        sqlx::query_as!(
            Donation,
            "INSERT INTO donations (amount, title, description, webhook, user_id) VALUES ($1, $2, $3, $4, $5) RETURNING *",
            self.amount,
            self.title,
            self.description,
            self.webhook,
            user_id,
        )
            .fetch_one(db)
            .await
    }

    pub async fn get(id: Uuid, user_id: Uuid, db: &PgPool) -> Result<Donation, Error> {
        sqlx::query_as!(
            Donation, "SELECT * FROM donations WHERE id = $1 AND user_id = $2", id, user_id
        )
            .fetch_one(db)
            .await
    }

    pub async fn list(user_id: Uuid, db: &PgPool) -> Result<Vec<Donation>, Error> {
        sqlx::query_as!(
            Donation, "SELECT * FROM donations WHERE user_id = $1", user_id
        )
            .fetch_all(db)
            .await
    }

    pub async fn delete(id: Uuid, user_id: Uuid, db: &PgPool) -> Result<PgQueryResult, Error> {
        sqlx::query!(
            "DELETE FROM donations WHERE id = $1 AND user_id = $2", id, user_id
        )
            .execute(db)
            .await
    }

    pub async fn update(self, id: Uuid, user_id: Uuid, db: &PgPool) -> Result<Donation, Error> {
        sqlx::query_as!(
            Donation,
            "
            UPDATE donations
            SET amount = $1, title = $2, description = $3, webhook = $4
            WHERE id = $5 AND user_id = $6
            RETURNING *
            ",
            self.amount,
            self.title,
            self.description,
            self.webhook,
            id,
            user_id
        )
            .fetch_one(db)
            .await
    }
}

pub async fn get_connection(db_url: &str) -> Result<PgPool, Error> {
    PgPool::connect(db_url).await
}

#[derive(Clone, Debug)]
pub struct User {
    pub id: Uuid,
    // pub email: String,
}

impl User {
    pub async fn get(id: Uuid) -> Result<User, Error> {
        Ok(Self {
            id,
            // email: "admin".to_string()
        })
    }
}
