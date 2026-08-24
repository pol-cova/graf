#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArxivPaper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub summary: String,
    pub year: u32,
    pub primary_category: String,
    pub pdf_url: String,
}

impl ArxivPaper {
    pub fn citekey(&self) -> String {
        let first_author = self
            .authors
            .first()
            .and_then(|a| a.split_whitespace().last())
            .unwrap_or("paper")
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();

        let first_word = self
            .title
            .split_whitespace()
            .find(|w| {
                let low = w.to_lowercase();
                !matches!(
                    low.as_str(),
                    "a" | "an" | "the" | "on" | "in" | "for" | "towards"
                )
            })
            .unwrap_or("paper")
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();

        format!("{}{first_word}", self.year)
            .chars()
            .fold(first_author.to_string(), |mut acc, c| {
                acc.push(c);
                acc
            })
    }

    pub fn to_bibtex(&self) -> String {
        let key = self.citekey();
        let authors_str = self.authors.join(" and ");
        let clean_id = self.id.trim_start_matches("http://arxiv.org/abs/");

        let mut out = format!("@article{{{key},\n");
        out.push_str(&format!("  author = {{{authors_str}}},\n"));
        out.push_str(&format!("  title = {{{}}},\n", self.title));
        out.push_str(&format!(
            "  journal = {{arXiv preprint arXiv:{clean_id}}},\n"
        ));
        out.push_str(&format!("  year = {{{}}},\n", self.year));
        out.push_str(&format!("  eprint = {{{clean_id}}},\n"));
        out.push_str("  archivePrefix = {arXiv},\n");
        if !self.primary_category.is_empty() {
            out.push_str(&format!(
                "  primaryClass = {{{}}},\n",
                self.primary_category
            ));
        }
        out.push_str("}\n");
        out
    }
}

pub fn parse_arxiv_atom_feed(xml: &str) -> Vec<ArxivPaper> {
    let mut papers = Vec::new();

    for entry_block in xml.split("<entry>") {
        if !entry_block.contains("</entry>") {
            continue;
        }

        let entry_xml = entry_block.split("</entry>").next().unwrap_or("");

        let id = extract_xml_tag(entry_xml, "id")
            .unwrap_or_default()
            .trim()
            .to_string();
        let title = extract_xml_tag(entry_xml, "title")
            .unwrap_or_default()
            .replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let summary = extract_xml_tag(entry_xml, "summary")
            .unwrap_or_default()
            .replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        let published = extract_xml_tag(entry_xml, "published").unwrap_or_default();
        let year = published
            .split('-')
            .next()
            .and_then(|y| y.parse::<u32>().ok())
            .unwrap_or(2024);

        let mut authors = Vec::new();
        for author_block in entry_xml.split("<author>") {
            if let Some(name) = extract_xml_tag(author_block, "name") {
                let clean_name = name.trim().to_string();
                if !clean_name.is_empty() {
                    authors.push(clean_name);
                }
            }
        }

        let clean_id = id
            .split("/abs/")
            .nth(1)
            .or_else(|| id.split('/').next_back())
            .unwrap_or(&id)
            .to_string();

        let pdf_url = format!("https://arxiv.org/pdf/{clean_id}.pdf");

        if !title.is_empty() {
            papers.push(ArxivPaper {
                id: clean_id,
                title,
                authors,
                summary,
                year,
                primary_category: "cs.AI".to_string(),
                pdf_url,
            });
        }
    }

    papers
}

fn extract_xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open_tag = format!("<{tag}>");
    let close_tag = format!("</{tag}>");

    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml[start..].find(&close_tag)? + start;
    Some(&xml[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arxiv_atom_feed() {
        let sample_xml = r#"
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/1706.03762v7</id>
    <published>2017-06-12T17:57:34Z</published>
    <title>Attention Is All You Need</title>
    <summary>The dominant sequence transduction models are based on complex recurrent neural networks.</summary>
    <author>
      <name>Ashish Vaswani</name>
    </author>
    <author>
      <name>Noam Shazeer</name>
    </author>
  </entry>
</feed>
"#;

        let papers = parse_arxiv_atom_feed(sample_xml);
        assert_eq!(papers.len(), 1);

        let paper = &papers[0];
        assert_eq!(paper.id, "1706.03762v7");
        assert_eq!(paper.title, "Attention Is All You Need");
        assert_eq!(paper.authors.len(), 2);
        assert_eq!(paper.year, 2017);

        let bibtex = paper.to_bibtex();
        assert!(bibtex.contains("@article{vaswani2017attention,"));
        assert!(bibtex.contains("author = {Ashish Vaswani and Noam Shazeer},"));
        assert!(bibtex.contains("journal = {arXiv preprint arXiv:1706.03762v7},"));
    }
}
