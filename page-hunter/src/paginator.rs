#[cfg(feature = "sqlite-sqlx")]
use sqlx::{query_scalar, sqlite::SqliteRow, Error, FromRow, QueryBuilder, Sqlite, SqlitePool};
use std::fmt::Debug;
#[cfg(feature = "sqlite-sqlx")]
use std::marker::PhantomData;

#[cfg(feature = "sqlite-sqlx")]
/// Paginator struct
///
/// - **query**: a [`QueryBuilder`] for a [`Sqlite`] database. This is what the pagination s based on.
/// - **pool**: a [`SqlitePool`] which is used to query the database
/// - **row**: [`PhantomData`] is used to keep track of the row type
pub struct SqlitePaginator<'a, 'q, S>
where
    S: for<'r> FromRow<'r, SqliteRow> + Clone,
{
    pub query: QueryBuilder<'q, Sqlite>,
    pub pool: &'a SqlitePool,
    pub(crate) row: PhantomData<S>,
    pub(crate) n: usize,
    pub(crate) page_size: usize,
    pub(crate) n_pages: usize,
    pub(crate) cur_page: usize,
    pub(crate) next_page: Option<usize>,
    pub(crate) prev_page: Option<usize>,
}

impl<'a, 'q, S> Debug for SqlitePaginator<'a, 'q, S>
where
    S: for<'r> FromRow<'r, SqliteRow> + Clone + Send + Unpin,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlitePaginator")
            .field("query", &self.query.sql())
            .field("row", &self.row)
            .field("n", &self.n)
            .field("page_size", &self.page_size)
            .field("n_pages", &self.n_pages)
            .field("cur_page", &self.cur_page)
            .field("next_page", &self.next_page)
            .field("prev_page", &self.prev_page)
            .finish()
    }
}

#[cfg(feature = "sqlite-sqlx")]
impl<'a, 'q, S> SqlitePaginator<'a, 'q, S>
where
    S: for<'r> FromRow<'r, SqliteRow> + Clone + Send + Unpin,
{
    /// Get the number of rows that will be paginated over
    pub fn n(&self) -> usize {
        self.n
    }

    /// Get the maximum page size
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Get the total number of pages
    pub fn n_pages(&self) -> usize {
        self.n_pages
    }

    /// Get the current page number (0 indexed)
    pub fn cur_page(&self) -> usize {
        self.cur_page
    }

    /// Get the next page number
    pub fn next_page(&self) -> Option<usize> {
        self.next_page
    }

    /// Get the previous page number
    pub fn prev_page(&self) -> Option<usize> {
        self.prev_page
    }

    /// Create a new [`SqlitePaginator`]
    pub async fn new(
        pool: &'a SqlitePool,
        query: QueryBuilder<'q, Sqlite>,
        page_size: usize,
    ) -> Result<Self, sqlx::Error> {
        // Create a query to count the number of pages
        let count_query = format!("select count(*) from ({})", query.sql());

        // Count the number of observations (fetches the count)
        let n: i64 = query_scalar(&count_query).fetch_one(pool).await?;
        let n = n as usize;

        // Calculate the number of pages
        let n_pages = ((n as f64) / (page_size as f64)).ceil() as usize;

        let paginator = Self {
            query,
            pool,
            row: PhantomData,
            n,
            page_size,
            n_pages,
            cur_page: 0_usize,
            next_page: if n > page_size { Some(1) } else { None },
            prev_page: None,
        };

        Ok(paginator)
    }

    // Implement methods to get data based on pagination state
    pub async fn fetch_page(&mut self, page: usize) -> Result<Vec<S>, sqlx::Error> {
        if page >= self.n_pages {
            return Err(Error::RowNotFound);
        }

        self.cur_page = page;
        self.next_page = if page + 1 < self.n_pages {
            Some(page + 1)
        } else {
            None
        };
        self.prev_page = if page > 0 { Some(page - 1) } else { None };

        let offset = page * self.page_size;

        // Modify the query to include the LIMIT and OFFSET
        self.query
            .reset()
            .push(" LIMIT ")
            .push_bind(self.page_size as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        // Execute the query and collect the results
        let rows = self
            .query
            .build_query_as::<S>()
            .fetch_all(self.pool)
            .await?;

        Ok(rows)
    }

    /// Fetches the current page
    pub async fn fetch_cur_page(&mut self) -> Result<Vec<S>, sqlx::Error> {
        self.fetch_page(self.cur_page).await
    }

    /// Fetches the next page if avaialable. Increments `cur_page` if successful.
    pub async fn fetch_next_page(&mut self) -> Result<Vec<S>, sqlx::Error> {
        match self.next_page {
            Some(p) => self.fetch_page(p).await,
            None => Err(Error::RowNotFound),
        }
    }
    /// Fetches the previous page if available. Decrements `cur_page` if successful.
    pub async fn fetch_prev_page(&mut self) -> Result<Vec<S>, sqlx::Error> {
        match self.prev_page {
            Some(p) => self.fetch_page(p).await,
            None => Err(Error::RowNotFound),
        }
    }

    pub async fn fetch_all_pages(&mut self) -> Result<Vec<S>, sqlx::Error> {
        let mut res_vec = Vec::with_capacity(self.n);
        for p in 0..self.n_pages {
            let page_res = self.fetch_page(p).await?;
            res_vec.extend(page_res);
        }
        Ok(res_vec)
    }
}
