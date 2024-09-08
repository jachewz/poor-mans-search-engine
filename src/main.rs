use once_cell::sync::Lazy;
use rand::Rng;
use redis::Commands;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;

static NON_WORDS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z0-9' ]").unwrap());

static STOP_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let stop_words = "a able about across after all almost also am
    among an and any are as at be because been but by can cannot
    could dear did do does either else ever every for from get got
    had has have he her hers him his how however i if in into is it
    its just least let like likely may me might most must my neither
    no nor not of off often on only or other our own rather said say
    says she should since so some than that the their them then there
    these they this tis to too twas us wants was we were what when
    where which while who whom why will with would yet you your";
    stop_words.split_whitespace().collect()
});

struct ScoredIndexSearch {
    prefix: String,
    connection: redis::Connection,
}

impl ScoredIndexSearch {
    fn new(prefix: &str, redis_url: &str) -> Self {
        let client = redis::Client::open(redis_url).unwrap();
        let connection = client.get_connection().unwrap();
        ScoredIndexSearch {
            prefix: format!("{}:", prefix.trim_end_matches(':').to_lowercase()),
            connection,
        }
    }

    fn get_index_keys(content: &str, add: bool) -> HashMap<String, f32> {
        let words: Vec<String> = NON_WORDS
            .replace_all(&content.to_lowercase(), " ")
            .split_whitespace()
            .map(|word| word.trim_matches('\'').to_string())
            .filter(|word| !STOP_WORDS.contains(word.as_str()) && word.len() > 1)
            .collect();

        if !add {
            return HashMap::from_iter(words.into_iter().map(|word| (word, 0.0)));
        }

        let mut counts = HashMap::new();
        for word in words {
            *counts.entry(word).or_insert(0.0) += 1.0;
        }
        let wordcount = counts.values().sum::<f32>();
        counts
            .iter()
            .map(|(word, &count)| (word.clone(), count / wordcount))
            .collect()
    }

    // Add or remove an item from the index. add = true for add, false for remove.
    fn handle_content(&mut self, id: &str, content: &str, add: bool) -> usize {
        let keys = ScoredIndexSearch::get_index_keys(content, add);
        let prefix = &self.prefix;

        let mut pipe = redis::pipe();

        if add {
            pipe.sadd(format!("{}indexed:", prefix), id);
            for (key, value) in &keys {
                pipe.zadd(format!("{}{}", prefix, key), id, *value);
            }
        } else {
            pipe.srem(format!("{}indexed:", prefix), id);
            for key in keys.keys() {
                pipe.zrem(format!("{}{}", prefix, key), id);
            }
        }

        let _: Vec<f32> = pipe.query(&mut self.connection).unwrap();
        keys.len()
    }

    fn add_indexed_item(&mut self, id: &str, content: &str) -> usize {
        self.handle_content(id, content, true)
    }

    fn search(
        &mut self,
        query_string: &str,
        offset: usize,
        count: usize,
    ) -> (Vec<(String, f32)>, usize) {
        let keys: Vec<String> = ScoredIndexSearch::get_index_keys(query_string, false)
            .keys()
            .map(|key| format!("{}{}", self.prefix, key))
            .collect();

        if keys.is_empty() {
            return (vec![], 0);
        }

        let total_docs = self
            .connection
            .scard(format!("{}indexed:", self.prefix))
            .unwrap_or(1) as f32;

        let mut pipe = redis::pipe();
        for key in &keys {
            pipe.zcard(key);
        }
        let sizes: Vec<f32> = pipe.query(&mut self.connection).unwrap();

        let idfs: Vec<f32> = sizes
            .iter()
            .map(|&count| {
                if count == 0.0 {
                    0.0
                } else {
                    (total_docs / count).log2().max(0.0)
                }
            })
            .collect();

        let weights: Vec<(&str, f32)> = keys
            .iter()
            .zip(idfs.iter())
            .filter(|(_, &idf)| idf > 0.0)
            .map(|(key, &idf)| (key.as_str(), idf))
            .collect();

        if weights.is_empty() {
            return (vec![], 0);
        }

        let temp_key: String = format!("{}temp:{}", self.prefix, rand::thread_rng().gen::<u64>());
        let known: usize = self
            .connection
            .zunionstore_weights(&temp_key, &weights)
            .unwrap_or(0);

        let ids = self
            .connection
            .zrevrange_withscores(&temp_key, offset as isize, (offset + count - 1) as isize)
            .unwrap();

        let _: () = self.connection.del(&temp_key).unwrap();
        (ids, known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_basic() {
        let mut t = ScoredIndexSearch::new("unittest", "redis://127.0.0.1/");
        // get existing keys
        let keys: Vec<String> = t.connection.keys("unittest:*").unwrap();
        // convert the keys to a slice of string references
        let key_refs: Vec<&str> = keys.iter().map(|key| key.as_str()).collect();
        // delete all keys
        //
        for key in key_refs {
            let _: () = t.connection.del(key).unwrap();
        }

        t.add_indexed_item("1", "hello world");
        t.add_indexed_item("2", "this world is nice and you are really special");

        assert_eq!(t.search("hello", 0, 10), (vec![("1".to_string(), 0.5)], 1));
        assert_eq!(
            t.search("world", 0, 10),
            (vec![("2".to_string(), 0.0), ("1".to_string(), 0.0)], 2)
        );
        assert_eq!(t.search("this", 0, 10), (vec![], 0));
        assert_eq!(
            t.search("hello really special nice world", 0, 10),
            (vec![("2".to_string(), 0.75), ("1".to_string(), 0.5)], 2)
        );
    }
}

fn main() -> redis::RedisResult<()> {
    // Initialize the ScoredIndexSearch struct
    let mut searcher = ScoredIndexSearch::new("myprefix", "redis://127.0.0.1/");

    // Example usage of the handle_content function
    let id = "1";
    let content = "This is an example of some content. Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.";

    let num_content = searcher.add_indexed_item(id, content);
    println!("Number of content indexed: {}", num_content);

    // Example usage of the search function
    let query_string = "example";
    let offset = 0;
    let count = 10;

    let (ids, known) = searcher.search(query_string, offset, count);
    println!("Search results: {:?}", ids);
    println!("Number of documents known: {}", known);

    Ok(())
}
