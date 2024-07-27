use rust_decimal::Decimal;
use sqlx;
use sqlx::{Error, PgPool};
use sqlx::types::time::OffsetDateTime;


#[derive(Debug, sqlx::FromRow)]
pub struct Transaction {
    pub id: String,
    pub amount: Decimal,
    pub address: String,
    pub r#type: String,
    pub created_at: Option<OffsetDateTime>,
}

impl Transaction {
    pub async fn create(self, db: &PgPool) -> Result<Transaction, Error> {
        sqlx::query_as!(
            Transaction,
            "
            INSERT
            INTO transactions (id, amount, address, type)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            ",
            self.id,
            self.amount,
            self.address,
            self.r#type,
        )
            .fetch_one(db)
            .await
    }

    pub async fn list(address: &str, db: &PgPool) -> Result<Vec<Transaction>, Error> {
        sqlx::query_as!(
            Transaction, "SELECT * FROM transactions WHERE address = $1", address
        )
            .fetch_all(db)
            .await
    }

    pub async fn sum(address: &str, db: &PgPool) -> Result<Decimal, Error> {
        let rows = sqlx::query!(
        r#"
        SELECT
            SUM(CASE WHEN type = 'income' THEN amount ELSE -amount END) AS total
        FROM transactions
        WHERE address = $1
        "#,
        address
    )
            .fetch_one(db)
            .await?;

        Ok(rows.total.unwrap_or_default())
    }
}

pub async fn get_connection(db_url: &str) -> Result<PgPool, Error> {
    PgPool::connect(db_url).await
}
