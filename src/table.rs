//! This module contains the table logic for the application.
//!
//! The main entry point is the [`Table`] struct which represents a table.
//! It also contains the [`IntoTable`] trait which allows any type to be converted into a table.
use std::{fmt::Display, iter};

/// Table representation.
///
/// A table is a collection of rows and columns.
///
/// The table is printed using the [`Display`] trait.
/// It's following the same format as the tables printed using the Atlas CLI.
pub struct Table {
    /// Header of the table.
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Column definition.
///
/// A column is a tuple of a name and a function to get the value of the column for a given item.
pub type TableColumn<S, T> = (S, fn(&T) -> String);

impl Table {
    /// Create a new table.
    ///
    /// # Arguments
    ///
    /// * `header` - The header of the table.
    /// * `rows` - The rows of the table.
    pub fn new(header: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self { header, rows }
    }

    /// Create a new table from an iterator of items.
    ///
    /// # Arguments
    ///
    /// * `iter` - The iterator of items.
    /// * `columns` - The columns of the table.
    pub fn from_iter<'a, S, Iter, Item>(iter: Iter, columns: &[TableColumn<S, Item>]) -> Self
    where
        S: Display,
        Iter: IntoIterator<Item = &'a Item>,
        Item: 'a,
    {
        // Convert the iter into an actual iterator.
        let iter = iter.into_iter();

        // Create the header from the column names.
        let header = columns.iter().map(|(name, _)| name.to_string()).collect();

        // Create the rows from the items and columns.
        let rows = iter
            .map(|item| columns.iter().map(|(_, f)| f(item)).collect())
            .collect();

        // Create the table.
        Self::new(header, rows)
    }
}

impl Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // calculate the max width of each column
        // combine the header and rows into a single iterator
        // then get the length of each cell + 4 (for the padding)
        let max_column_widths: Vec<usize> = iter::once(&self.header)
            .chain(self.rows.iter())
            .map(|row| row.iter().map(|cell| cell.len() + 4).collect())
            .max()
            .unwrap_or_default();

        // print the rows with tab separation
        for row in iter::once(&self.header).chain(self.rows.iter()) {
            // print the cells with fixed widths
            for (cell, width) in row.iter().zip(max_column_widths.iter()) {
                write!(f, "{:<width$}", cell, width = width)?;
            }

            // end the row
            writeln!(f)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test data structures
    struct Person {
        name: String,
        age: u32,
        city: String,
    }

    struct Product {
        id: u32,
        name: String,
        price: f64,
    }

    #[test]
    fn test_from_iter_single_row() {
        let person = Person {
            name: "Alice".to_string(),
            age: 30,
            city: "New York".to_string(),
        };

        let columns: &[TableColumn<&str, Person>] = &[
            ("Name", |p: &Person| p.name.clone()),
            ("Age", |p: &Person| p.age.to_string()),
            ("City", |p: &Person| p.city.clone()),
        ];

        let table = Table::from_iter([&person], columns);
        let output = format!("{}", table);

        assert_eq!(
            output,
            "Name     Age   City        \nAlice    30    New York    \n"
        );
    }

    #[test]
    fn test_from_iter_multiple_rows() {
        let people = vec![
            Person {
                name: "Alice".to_string(),
                age: 30,
                city: "New York".to_string(),
            },
            Person {
                name: "Bob".to_string(),
                age: 25,
                city: "London".to_string(),
            },
            Person {
                name: "Charlie".to_string(),
                age: 35,
                city: "Paris".to_string(),
            },
        ];

        let columns: &[TableColumn<&str, Person>] = &[
            ("Name", |p: &Person| p.name.clone()),
            ("Age", |p: &Person| p.age.to_string()),
            ("City", |p: &Person| p.city.clone()),
        ];

        let table = Table::from_iter(people.iter(), columns);
        let output = format!("{}", table);

        let expected = "Name       Age   City     \nAlice      30    New York \nBob        25    London   \nCharlie    35    Paris    \n";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_from_iter_empty_iterator() {
        let people: Vec<Person> = vec![];

        let columns: &[TableColumn<&str, Person>] = &[
            ("Name", |p: &Person| p.name.clone()),
            ("Age", |p: &Person| p.age.to_string()),
        ];

        let table = Table::from_iter(people.iter(), columns);
        let output = format!("{}", table);

        // Should only have the header, no rows
        assert_eq!(output, "Name    Age    \n");
    }

    #[test]
    fn test_from_iter_single_column() {
        let products = vec![
            Product {
                id: 1,
                name: "Widget".to_string(),
                price: 9.99,
            },
            Product {
                id: 2,
                name: "Gadget".to_string(),
                price: 19.99,
            },
        ];

        let columns: &[TableColumn<&str, Product>] = &[("Name", |p: &Product| p.name.clone())];

        let table = Table::from_iter(products.iter(), columns);
        let output = format!("{}", table);

        assert_eq!(output, "Name      \nWidget    \nGadget    \n");
    }

    #[test]
    fn test_from_iter_numeric_types() {
        let products = vec![
            Product {
                id: 1,
                name: "Widget".to_string(),
                price: 9.99,
            },
            Product {
                id: 2,
                name: "Gadget".to_string(),
                price: 19.99,
            },
        ];

        let columns: &[TableColumn<&str, Product>] = &[
            ("ID", |p: &Product| p.id.to_string()),
            ("Name", |p: &Product| p.name.clone()),
            ("Price", |p: &Product| p.price.to_string()),
        ];

        let table = Table::from_iter(products.iter(), columns);
        let output = format!("{}", table);

        assert_eq!(
            output,
            "ID    Name    Price    \n1     Widget  9.99     \n2     Gadget  19.99    \n"
        );
    }
}
