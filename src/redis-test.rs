use redis::{Commands, RedisResult};

fn main() -> RedisResult<()> {
    // Connect to the redis server
    let client = redis::Client::open("redis://127.0.0.1")?;
    let mut con = client.get_connection()?;

    let _: () = con.sadd("temp_1", "1")?;

    println!("added 1 to temp_1");

    Ok(())
}
