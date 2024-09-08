use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};

use rand::random;

use redis::{Commands, RedisResult, Pipeline};

static NON_WORDS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-z0-9' ]").unwrap());

static STOP_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let stop_words = "a able about across after all almost also am
    among an and any are as at be because been but by can cannot
    could dear did do does either else ever every for from get got
    had has have he her hers him his how however i if in into is it
    its just least let like likely may me might most must my neither
    no nor not of off often on only or other our own rather said say
    says she should since so some than that the their them then
    there these they this tis to too twas us wants was we were what
    when where which while who whom why will with would yet you
    your";
    stop_words.split_whitespace().collect()
});

fn get_index_keys(content: &str, add: bool) -> HashMap<String, f32> {
    // Apply the regex to replace non-word characters with spaces and convert to lowercase
    let words: Vec<String> = NON_WORDS
        .replace_all(&content.to_lowercase(), " ")
        .split_whitespace()
        .map(|word| word.trim_matches('\'').to_string())
        .filter(|word| !STOP_WORDS.contains(word.as_str()) && word.len() > 1)
        .collect();

    // apply Porter Stemmer here if you want to
    // apply Metaphone/Double Metaphone here if you want to

    if !add {
        words.into_iter().map(|w| (w, 0.0)).collect()
    } else {
        // Calculate the term frequency (TF) portion of TF/IDF
        let word_count = words.len();
        let mut counts: HashMap<String, f32> = HashMap::new();

        for word in words {
            *counts.entry(word).or_insert(0.0) += 1.0;
        }

        // Normalize the counts
        counts
            .iter_mut()
            .for_each(|(_, count)| *count /= word_count as f32);

        counts
    }
}

fn handle_content(con: &mut redis::Connection, prefix: &str, id: &str, content: &str, add: bool) -> redis::RedisResult<usize> {
    let keys = get_index_keys(content, true);

    let mut pipe: Pipeline = redis::pipe();
    let set_key = format!("{}indexed:", prefix);

    // print keys
    print!("Keys: ");
    for (word, freq) in &keys {
        println!("{}: {}", word, freq);
    }

    if add {
        pipe.sadd(set_key, id);
        for (word, freq) in &keys {
            pipe.zadd(format!("{}{}", prefix, word), id, freq);
        }
        
    } else {
        pipe.srem(set_key, id);
        for word in keys.keys() {
            pipe.zrem(format!("{}{}", prefix, word), id);
        }
    }

    pipe.query(con)?;

    // Return the number of keys processed
    Ok(keys.len())
}

// Calculate the inverse document frequency (IDF) values
fn idf(count: u64, total_docs: u64) -> f64 {
    if count == 0 {
        0.0 // Avoid division by zero
    } else {
        (total_docs as f64 / count as f64).log2().max(0.0)
    }
}

fn search(con: &mut redis::Connection, prefix: &str, query_string: &str, offset: usize, count: usize
) -> RedisResult<(Vec<(String, f64)>, u64)> {
    let keys: Vec<String> = get_index_keys(query_string, false)
    .into_iter()
    .map(|(key, _)| format!("{}:{}", prefix, key))
    .collect();

    if keys.is_empty() {
        return Ok((vec![], 0));
    }

    let total_docs: u64 = con.scard::<_, u64>(format!("{}indexed:", prefix))?.max(1);

    // Get our document frequency values
    let mut pipe = redis::pipe();
    for key in &keys {
        pipe.zcard(key);
    }
    let sizes: Vec<u64> = pipe.query(con)?;

    // Calculate the inverse document frequency (IDF) values
    let idfs: Vec<f64> = sizes
        .into_iter()
        .map(|size| idf(size, total_docs))
        .collect();

    // Create the weights as a vector of tuples to pass to ZUNIONSTORE
    let weights: Vec<(&str, f64)> = keys
    .iter()
    .zip(idfs.iter())
    .filter(|(_, &idf)| idf > 0.0)
    .map(|(key, &idf)| (key.as_str(), idf))
    .collect();
    
    if weights.is_empty() {
        return Ok((vec![], 0));
    }

    // Generate a temporary key to store the union results
    let temp_key = format!("{}temp:{:x}", prefix, random::<u8>());
    
    // Perform the union
    let known: u64 = con.zunionstore_weights(&temp_key,  &weights)?;

    // Get the results
    let ids = con.zrevrange_withscores(&temp_key, offset as isize, (offset + count) as isize)?;

    // Clean up the temporary key
    con.del(&temp_key)?;

    Ok((ids, known))

}

fn main() -> redis::RedisResult<()> {
    // Connect to Redis server
    let client = redis::Client::open("redis://127.0.0.1/")?;
    let mut connection = client.get_connection()?;

    // Example usage of the handle_content function
    let prefix = "myprefix";
    let id = "1";
    let content = "This is an example of some content. Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.";
    let num_content = handle_content(&mut connection, prefix, id, content, true)?;
    println!("Number of content indexed: {}", num_content);

    // Example usage of the search function
    let prefix = "myprefix";
    let query_string = "example";
    let offset = 0;
    let count = 10;

    // let (ids, known) = search(&mut connection, prefix, query_string, offset, count)?;

    // println!("Search results: {:?}", ids);
    // println!("Number of documents known: {}", known);

    Ok(())
}