// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct BookInsertParams {
    pub tenant_id: i64,
    pub session_id: i64,
    pub customer_id: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Lookup {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub starts_at: chrono::DateTime<chrono::FixedOffset>,
    pub capacity: i32,
    pub booked: i32,
    pub coach: String,
    pub room: String,
}
pub struct LookupBorrowed<'a> {
    pub id: i64,
    pub tenant_id: i64,
    pub name: &'a str,
    pub starts_at: chrono::DateTime<chrono::FixedOffset>,
    pub capacity: i32,
    pub booked: i32,
    pub coach: &'a str,
    pub room: &'a str,
}
impl<'a> From<LookupBorrowed<'a>> for Lookup {
    fn from(
        LookupBorrowed {
            id,
            tenant_id,
            name,
            starts_at,
            capacity,
            booked,
            coach,
            room,
        }: LookupBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            tenant_id,
            name: name.into(),
            starts_at,
            capacity,
            booked,
            coach: coach.into(),
            room: room.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub starts_at: chrono::DateTime<chrono::FixedOffset>,
    pub capacity: i32,
    pub booked: i32,
    pub coach: String,
    pub room: String,
}
pub struct ListBorrowed<'a> {
    pub id: i64,
    pub tenant_id: i64,
    pub name: &'a str,
    pub starts_at: chrono::DateTime<chrono::FixedOffset>,
    pub capacity: i32,
    pub booked: i32,
    pub coach: &'a str,
    pub room: &'a str,
}
impl<'a> From<ListBorrowed<'a>> for List {
    fn from(
        ListBorrowed {
            id,
            tenant_id,
            name,
            starts_at,
            capacity,
            booked,
            coach,
            room,
        }: ListBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            tenant_id,
            name: name.into(),
            starts_at,
            capacity,
            booked,
            coach: coach.into(),
            room: room.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&str, tokio_postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct LookupQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<LookupBorrowed, tokio_postgres::Error>,
    mapper: fn(LookupBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LookupQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LookupBorrowed) -> R) -> LookupQuery<'c, 'a, 's, C, R, N> {
        LookupQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct ListQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ListBorrowed, tokio_postgres::Error>,
    mapper: fn(ListBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ListBorrowed) -> R) -> ListQuery<'c, 'a, 's, C, R, N> {
        ListQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct SetTenantStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_tenant() -> SetTenantStmt {
    SetTenantStmt("SELECT set_config('app.tenant_id', $1, true)", None)
}
impl SetTenantStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        tenant: &'a T1,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [tenant],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct LookupStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn lookup() -> LookupStmt {
    LookupStmt(
        "SELECT id, tenant_id, name, starts_at, capacity, booked, coach, room FROM class_session WHERE id = $1",
        None,
    )
}
impl LookupStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        id: &'a i64,
    ) -> LookupQuery<'c, 'a, 's, C, Lookup, 1> {
        LookupQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<LookupBorrowed, tokio_postgres::Error> {
                    Ok(LookupBorrowed {
                        id: row.try_get(0)?,
                        tenant_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        starts_at: row.try_get(3)?,
                        capacity: row.try_get(4)?,
                        booked: row.try_get(5)?,
                        coach: row.try_get(6)?,
                        room: row.try_get(7)?,
                    })
                },
            mapper: |it| Lookup::from(it),
        }
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "SELECT id, tenant_id, name, starts_at, capacity, booked, coach, room FROM class_session WHERE starts_at > $1 ORDER BY starts_at LIMIT 50",
        None,
    )
}
impl ListStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        since: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> ListQuery<'c, 'a, 's, C, List, 1> {
        ListQuery {
            client,
            params: [since],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<ListBorrowed, tokio_postgres::Error> {
                Ok(ListBorrowed {
                    id: row.try_get(0)?,
                    tenant_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    starts_at: row.try_get(3)?,
                    capacity: row.try_get(4)?,
                    booked: row.try_get(5)?,
                    coach: row.try_get(6)?,
                    room: row.try_get(7)?,
                })
            },
            mapper: |it| List::from(it),
        }
    }
}
pub struct BookUpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn book_update() -> BookUpdateStmt {
    BookUpdateStmt(
        "UPDATE class_session SET booked = booked + 1 WHERE id = $1",
        None,
    )
}
impl BookUpdateStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        id: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
pub struct BookInsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn book_insert() -> BookInsertStmt {
    BookInsertStmt(
        "INSERT INTO booking (tenant_id, session_id, customer_id) VALUES ($1, $2, $3)",
        None,
    )
}
impl BookInsertStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        tenant_id: &'a i64,
        session_id: &'a i64,
        customer_id: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[tenant_id, session_id, customer_id])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        BookInsertParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for BookInsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a BookInsertParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.tenant_id,
            &params.session_id,
            &params.customer_id,
        ))
    }
}
