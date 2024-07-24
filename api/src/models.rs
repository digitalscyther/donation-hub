use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
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
            id: self.id.map_or_else(Uuid::new_v4, |id_str| Uuid::parse_str(&id_str).unwrap_or(Uuid::new_v4())),    // TODO
            amount: self.amount,
            title: self.title,
            description: self.description,
            webhook: self.webhook,
            wallet_id: self.wallet_id.map_or(None, |id_str| Some(Uuid::parse_str(&id_str).unwrap())),    // TODO
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
            "INSERT INTO donations (amount, title, description, webhook, wallet_id, user_id) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
            self.amount,
            self.title,
            self.description,
            self.webhook,
            self.wallet_id,
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

#[derive(Serialize, Deserialize)]
pub struct WalletData {
    pub address: String,
    pub private_key: Option<String>,    // TODO cypher
}

pub struct Wallet {
    pub id: Uuid,
    pub data: WalletData,
    pub is_active: bool,
    pub user_id: Uuid,
}

pub struct WalletRow {
    pub id: Uuid,
    pub data: Value,
    pub is_active: bool,
    pub user_id: Uuid,
}

impl From<Value> for WalletData {
    fn from(value: Value) -> Self {
        serde_json::from_value(value).unwrap()
    }
}

impl From<WalletData> for Value {
    fn from(value: WalletData) -> Self {
        serde_json::to_value(value).unwrap()
    }
}

impl From<WalletRow> for Wallet {
    fn from(row: WalletRow) -> Self {
        Wallet {
            id: row.id,
            data: row.data.into(),
            is_active: row.is_active,
            user_id: row.user_id
        }
    }
}

impl Wallet {
    pub async fn create(
        pool: &PgPool, address: String, private_key: Option<String>, user_id: Uuid
    ) -> Result<Wallet, Error> {
        let data: Value = json!(WalletData { address, private_key });

        let row = sqlx::query!(
           "
           INSERT INTO wallets (data, user_id)
           VALUES ($1, $2)
           RETURNING id, data, is_active, user_id
           ",
           data, user_id
        )
            .fetch_one(pool)
            .await?;

        let wallet_data: WalletData = serde_json::from_value(row.data).unwrap();

        Ok(Wallet {
            id: row.id,
            data: wallet_data,
            is_active: row.is_active,
            user_id: row.user_id,
        })
    }

    pub async fn get(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<Wallet, Error> {
        let row = sqlx::query_as!(
            WalletRow, "SELECT * FROM wallets WHERE id = $1 AND user_id = $2", id, user_id
        )
            .fetch_one(pool)
            .await?;

        Ok(row.into())
    }
}
