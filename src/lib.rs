use std::collections::HashMap;

struct Document {
    content: String,
    nterms: i32, // number of terms (filtered words) in the document
}

pub struct Searcher {
    index: HashMap<String, HashMap<String, i32>>, // term -> doc_id -> count
    docs: HashMap<String, Document>,              // doc_id -> document
    avdl: f32,                                    // average document length

    k1: f32, // limits the impact of term frequency for BM25
    b: f32,  // document length normalization parameter for BM25
}

/// Normalize a string by removing non-alphanumeric characters, converting to lowercase, and removing stop words.
fn normalize_string(s: &str) -> String {
    let stop_words_eng = stop_words::get(stop_words::LANGUAGE::English);
    let non_words_re = regex::Regex::new(r"[^a-z0-9 ]").unwrap();

    non_words_re
        .replace_all(&s.to_lowercase(), " ")
        .split_whitespace()
        .filter(|word| !stop_words_eng.contains(&word.to_string()))
        .collect::<Vec<&str>>()
        .join(" ")
}

impl Default for Searcher {
    fn default() -> Self {
        Searcher::new()
    }
}

impl Searcher {
    pub fn new() -> Searcher {
        Searcher {
            index: HashMap::new(),
            docs: HashMap::new(),
            avdl: 0.0,

            k1: 1.2,
            b: 0.75,
        }
    }

    pub fn add_document(&mut self, doc_id: &str, doc_content: &str) {
        let filtered_content = normalize_string(doc_content);
        let mut nterms = 0;

        // map the number of times each term appears in the document
        for term in filtered_content.split_whitespace() {
            nterms += 1;
            let term = term.to_string();
            let doc_index = self.index.entry(term).or_default();
            doc_index.entry(doc_id.to_string()).and_modify(|x| *x += 1).or_insert(1);
        }

        self.docs.insert(
            doc_id.to_string(),
            Document {
                content: doc_content.to_string(),
                nterms,
            },
        );

        // recalculate the average document length
        self.avdl =
            (self.avdl * (self.docs.len() - 1) as f32 + nterms as f32) / self.docs.len() as f32;
    }

    /// Receives a query, normalizes it, gets a score for each query term and returns a hashmap of doc_id -> total score
    pub fn search(&self, query: &str) -> HashMap<String, f32> {
        let normalized_query = normalize_string(query);
        normalized_query
            .split_whitespace()
            .map(|term| self.bm25(term))
            .fold(HashMap::new(), |mut acc, scores| {
                for (doc_id, score) in scores {
                    let total_score = acc.entry(doc_id).or_insert(0.0);
                    *total_score += score;
                }
                acc
            })
    }

    fn idf(&self, term: &str) -> f32 {
        let docs_count = self.docs.len() as f32;

        
        let docs_with_term_count = match self.index.get(term) {
            None => 0 as f32,
            Some(docs) => docs.len() as f32,
        };
    
        // idf smooth variant
        ((docs_count - docs_with_term_count + 0.5) / (docs_with_term_count + 0.5) + 1.0).ln()
    }

    fn bm25(&self, term: &str) -> HashMap<String, f32> {
        match self.index.get(term) {
            None => HashMap::new(),
            Some(docs) => {
                let idf = self.idf(term);
                docs.iter()
                    .map(|(doc_id, count)| {
                        let doc = &self.docs[doc_id];
                        let tf = *count as f32;
                        let dl = doc.nterms as f32;

                        let numerator = tf * (self.k1 + 1.0);
                        let denominator = self.k1 * ((1.0 - self.b) + self.b * (dl / self.avdl));

                        (doc_id.to_string(), idf * numerator / denominator)
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_STRING: &str = "Nice, hello world! I like 42.";

    #[test]
    fn test_normalize_string() {
        assert_eq!(normalize_string(TEST_STRING), "nice 42".to_string());
    }

    #[test]
    fn test_add_document() {
        let mut searcher = Searcher::new();
        searcher.add_document("1", TEST_STRING);
        searcher.add_document("2", "");
        assert_eq!(searcher.docs.len(), 2);
        assert_eq!(searcher.docs["1"].nterms, 2);
    }

    #[test]
    fn test_search() {
        let mut searcher = Searcher::new();
        searcher.add_document("1", TEST_STRING);
        searcher.add_document("2", "Hello, moon!");
        searcher.add_document("3", "Hello, sun!");

        let results = searcher.search("moon sun");
        assert_eq!(results.len(), 2);
        assert!(results["2"] > 1.0);
        assert!(results["3"] > 1.0);
    }

    #[test]
    fn test_bm25() {
        let mut searcher = Searcher::new();
        searcher.add_document("1", "Hello, world!");
        searcher.add_document("2", "Hello, moon!");
        searcher.add_document("3", "Hello, sun!");

        assert_eq!(searcher.docs.len(), 3);

        let results = searcher.bm25("moon");
        assert_eq!(results.len(), 1);
        assert!(results["2"] > 1.0);
    }
}
