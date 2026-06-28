//! Interactive Visual Table & Matrix Builder engine.

/// Text alignment within a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

impl TableAlignment {
    /// Returns the LaTeX column specifier character (`l`, `c`, `r`).
    pub fn latex_spec(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "c",
            Self::Right => "r",
        }
    }

    /// Returns the Typst alignment identifier (`left`, `center`, `right`).
    pub fn typst_spec(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// Matrix bracket enclosure style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixStyle {
    Parentheses, // \begin{pmatrix}
    Brackets,    // \begin{bmatrix}
    Determinant, // \begin{vmatrix}
    None,        // \begin{matrix}
}

impl MatrixStyle {
    /// Returns the LaTeX environment name for this matrix style.
    pub fn latex_env(self) -> &'static str {
        match self {
            Self::Parentheses => "pmatrix",
            Self::Brackets => "bmatrix",
            Self::Determinant => "vmatrix",
            Self::None => "matrix",
        }
    }
}

/// Structured data model representing an academic table.
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
    /// Creates a new empty table with the given dimensions.
    pub fn new(num_rows: usize, num_cols: usize) -> Self {
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

    /// Parses clipboard text formatted as tab-separated values (TSV) from Excel / Sheets.
    pub fn from_tsv(tsv: &str) -> Self {
        let mut rows = Vec::new();
        let mut max_cols = 0;

        for line in tsv.lines() {
            let cols: Vec<String> = line.split('\t').map(|s| s.trim().to_string()).collect();
            if !cols.is_empty() {
                max_cols = max_cols.max(cols.len());
                rows.push(cols);
            }
        }

        if rows.is_empty() {
            return Self::new(2, 2);
        }

        for row in &mut rows {
            while row.len() < max_cols {
                row.push(String::new());
            }
        }

        let alignments = vec![TableAlignment::Left; max_cols];
        Self {
            rows,
            alignments,
            has_header: true,
            has_booktabs: true,
            caption: None,
            label: None,
        }
    }

    /// Parses clipboard text formatted as comma-separated values (CSV).
    pub fn from_csv(csv: &str) -> Self {
        let mut rows = Vec::new();
        let mut max_cols = 0;

        for line in csv.lines() {
            let cols: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
            if !cols.is_empty() {
                max_cols = max_cols.max(cols.len());
                rows.push(cols);
            }
        }

        if rows.is_empty() {
            return Self::new(2, 2);
        }

        for row in &mut rows {
            while row.len() < max_cols {
                row.push(String::new());
            }
        }

        let alignments = vec![TableAlignment::Left; max_cols];
        Self {
            rows,
            alignments,
            has_header: true,
            has_booktabs: true,
            caption: None,
            label: None,
        }
    }

    /// Generates high-quality publication LaTeX `booktabs` code.
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

        let col_specs: String = self.alignments.iter().map(|a| a.latex_spec()).collect();
        out.push_str(&format!("  \\begin{{tabular}}{{{col_specs}}}\n"));

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

    /// Generates native Typst `#figure(table(...))` code.
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
            if i == 0 && self.has_header {
                let header_cells: Vec<String> = row.iter().map(|c| format!("[*{c}*]")).collect();
                out.push_str(&format!("    table.header({}),\n", header_cells.join(", ")));
            } else {
                let row_cells: Vec<String> = row.iter().map(|c| format!("[{c}]")).collect();
                out.push_str(&format!("    {},\n", row_cells.join(", ")));
            }
        }

        out.push_str("  ),\n");
        if let Some(caption) = &self.caption {
            out.push_str(&format!("  caption: [{caption}],\n"));
        }
        out.push(')');

        if let Some(label) = &self.label {
            out.push_str(&format!(" <{label}>"));
        }
        out.push('\n');
        out
    }

    /// Generates LaTeX matrix markup for mathematical equations.
    pub fn to_matrix_latex(&self, style: MatrixStyle) -> String {
        let env = style.latex_env();
        let mut out = format!("\\begin{{{env}}}\n");
        for row in &self.rows {
            let row_str = row.join(" & ");
            out.push_str(&format!("  {row_str} \\\\\n"));
        }
        out.push_str(&format!("\\end{{{env}}}"));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsv_parsing_and_latex_export() {
        let tsv = "Model\tAccuracy\tF1 Score\nResNet-50\t92.4\t91.8\nTransformer\t96.8\t96.5";
        let mut table = TableData::from_tsv(tsv);
        table.caption = Some("Model Performance Comparison".to_string());
        table.label = Some("tab:models".to_string());
        table.alignments = vec![
            TableAlignment::Left,
            TableAlignment::Center,
            TableAlignment::Right,
        ];

        let latex = table.to_latex();
        assert!(latex.contains("\\begin{table}[htbp]"));
        assert!(latex.contains("\\caption{Model Performance Comparison}"));
        assert!(latex.contains("\\label{tab:models}"));
        assert!(latex.contains("\\begin{tabular}{lcr}"));
        assert!(latex.contains("\\toprule"));
        assert!(latex.contains("Model & Accuracy & F1 Score \\\\"));
        assert!(latex.contains("\\midrule"));
        assert!(latex.contains("Transformer & 96.8 & 96.5 \\\\"));
        assert!(latex.contains("\\bottomrule"));
    }

    #[test]
    fn test_csv_parsing_and_typst_export() {
        let csv = "Epoch,Loss,Accuracy\n1,0.45,88.2\n2,0.21,94.6";
        let mut table = TableData::from_csv(csv);
        table.caption = Some("Training Progress".to_string());
        table.label = Some("tab:training".to_string());

        let typst = table.to_typst();
        assert!(typst.contains("#figure("));
        assert!(typst.contains("table("));
        assert!(typst.contains("table.header([*Epoch*], [*Loss*], [*Accuracy*])"));
        assert!(typst.contains("[1], [0.45], [88.2]"));
        assert!(typst.contains("caption: [Training Progress]"));
        assert!(typst.contains("<tab:training>"));
    }

    #[test]
    fn test_latex_matrix_generation() {
        let mut table = TableData::new(2, 2);
        table.rows[0] = vec!["a".to_string(), "b".to_string()];
        table.rows[1] = vec!["c".to_string(), "d".to_string()];

        let matrix = table.to_matrix_latex(MatrixStyle::Brackets);
        assert_eq!(
            matrix,
            "\\begin{bmatrix}\n  a & b \\\\\n  c & d \\\\\n\\end{bmatrix}"
        );
    }
}
