#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
}

impl TableAlignment {
    pub fn latex_spec(self) -> &'static str {
        match self {
            Self::Left => "l",
        }
    }

    pub fn typst_spec(self) -> &'static str {
        match self {
            Self::Left => "left",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableData {
    pub rows: Vec<Vec<String>>,
    pub alignments: Vec<TableAlignment>,
    pub has_header: bool,
    pub has_booktabs: bool,
    pub caption: Option<String>,
    pub label: Option<String>,
}

impl TableData {
    #[cfg(test)]
    pub fn new(num_rows: usize, num_cols: usize) -> Self {
        Self::empty_grid(num_rows, num_cols)
    }

    fn empty_grid(num_rows: usize, num_cols: usize) -> Self {
        let rows = vec![vec![String::new(); num_cols]; num_rows];
        let alignments = vec![TableAlignment::Left; num_cols];
        Self {
            rows,
            alignments,
            has_header: true,
            has_booktabs: true,
            caption: None,
            label: None,
        }
    }

    /// The 3x3 seeded table the "Insert Table" command starts from.
    pub fn sample() -> Self {
        let mut table = Self::empty_grid(3, 3);
        table.rows[0] = vec![
            "Column 1".to_string(),
            "Column 2".to_string(),
            "Column 3".to_string(),
        ];
        table
    }

    pub fn to_latex(&self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }

        let mut out = String::new();
        out.push_str("\\begin{table}[htbp]\n");
        out.push_str("  \\centering\n");
        if let Some(caption) = &self.caption {
            out.push_str(&format!("  \\caption{{{caption}}}\n"));
        }
        if let Some(label) = &self.label {
            out.push_str(&format!("  \\label{{{label}}}\n"));
        }

        let cols_spec = self
            .alignments
            .iter()
            .map(|a| a.latex_spec())
            .collect::<String>();
        out.push_str(&format!("  \\begin{{tabular}}{{{cols_spec}}}\n"));

        if self.has_booktabs {
            out.push_str("    \\toprule\n");
        } else {
            out.push_str("    \\hline\n");
        }

        for (i, row) in self.rows.iter().enumerate() {
            let row_str = row.join(" & ");
            out.push_str(&format!("    {row_str} \\\\\n"));

            if i == 0 && self.has_header && self.rows.len() > 1 {
                if self.has_booktabs {
                    out.push_str("    \\midrule\n");
                } else {
                    out.push_str("    \\hline\n");
                }
            }
        }

        if self.has_booktabs {
            out.push_str("    \\bottomrule\n");
        } else {
            out.push_str("    \\hline\n");
        }

        out.push_str("  \\end{tabular}\n");
        out.push_str("\\end{table}\n");
        out
    }

    pub fn to_typst(&self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }

        let num_cols = self.alignments.len();
        let cols_spec = format!("({:?})", vec!["1fr"; num_cols].join(", ")).replace('"', "");
        let align_spec = format!(
            "({})",
            self.alignments
                .iter()
                .map(|a| a.typst_spec())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let mut out = String::new();
        out.push_str("#figure(\n");
        out.push_str(&format!(
            "  table(\n    columns: {cols_spec},\n    align: {align_spec},\n"
        ));

        for (i, row) in self.rows.iter().enumerate() {
            let header_mark = if i == 0 && self.has_header { "*" } else { "" };
            let cells = row
                .iter()
                .map(|cell| {
                    let trimmed = cell.trim();
                    format!("[{header_mark}{trimmed}{header_mark}]")
                })
                .collect::<Vec<_>>()
                .join(", ");
            if i == 0 && self.has_header {
                out.push_str(&format!("    table.header({cells}),\n"));
            } else {
                out.push_str(&format!("    {cells},\n"));
            }
        }
        out.push_str("  ),\n");

        if let Some(caption) = &self.caption {
            out.push_str(&format!("  caption: [{caption}],\n"));
        }
        if let Some(label) = &self.label {
            out.push_str(&format!("  <{label}>\n"));
        }
        out.push_str(")\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default 3x3 grid the workspace inserts and exports per engine.
    fn insertable_table() -> TableData {
        let mut table = TableData::new(3, 3);
        table.rows[0] = vec![
            "Column 1".to_string(),
            "Column 2".to_string(),
            "Column 3".to_string(),
        ];
        table
    }

    #[test]
    fn latex_export_shapes_a_booktabs_table() {
        let mut table = insertable_table();
        table.caption = Some("Demo".to_string());
        table.label = Some("tab:demo".to_string());

        let latex = table.to_latex();
        assert!(latex.contains("\\begin{table}[htbp]"));
        assert!(latex.contains("\\caption{Demo}"));
        assert!(latex.contains("\\label{tab:demo}"));
        assert!(latex.contains("\\begin{tabular}{lll}"));
        assert!(latex.contains("\\toprule"));
        assert!(latex.contains("Column 1 & Column 2 & Column 3 \\\\"));
        assert!(latex.contains("\\bottomrule"));
    }

    #[test]
    fn typst_export_uses_a_table_header_row() {
        let table = insertable_table();
        let typst = table.to_typst();
        assert!(typst.contains("#figure("));
        assert!(typst.contains("table.header([*Column 1*], [*Column 2*], [*Column 3*])"));
        assert!(typst.contains("[], [], []"));
    }

    #[test]
    fn empty_grid_exports_nothing() {
        assert!(TableData::new(0, 0).to_latex().is_empty());
        assert!(TableData::new(0, 0).to_typst().is_empty());
    }
}
