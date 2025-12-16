use std::fmt::Display;

pub struct Table {
    pub header: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}

pub type TableColumn<T> = (&'static str, fn(&T) -> String);

impl Table {
    pub fn new(header: Vec<&'static str>, rows: Vec<Vec<String>>) -> Self {
        Self { header, rows }
    }

    pub fn from_iter<'a, Iter, Item>(
        iter: Iter,
        columns: &[TableColumn<Item>],
    ) -> Self
    where
        Iter: IntoIterator<Item = &'a Item>,
        Item: 'a,
    {
        let iter = iter.into_iter();

        let header = columns.iter().map(|(name, _)| *name).collect();
        let rows = iter
            .map(|item| columns.iter().map(|(_, f)| f(item)).collect())
            .collect();

        Self::new(header, rows)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // print the headers with tab separation
        write!(f, "{}", self.header.join("\t"))?;

        // print the rows with tab separation
        for row in &self.rows {
            write!(f, "\n{}", row.join("\t"))?;
        }

        Ok(())
    }
}

pub trait IntoTable {
    fn as_table(&self) -> Table;
}
