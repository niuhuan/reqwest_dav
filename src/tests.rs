use crate::{Auth, ClientBuilder, Depth, Error};

#[tokio::test]
async fn it_works() -> Result<(), Error> {
    let client = ClientBuilder::new()
        .set_host("http://server/remote.php/dav/files/username/".to_string())
        .set_auth(Auth::Basic("username".to_owned(), "password".to_owned()))
        .build()?;

    println!(
        "{}",
        serde_json::to_string(&client.list("", Depth::Infinity).await?).unwrap()
    );

    client.delete("1.txt").await.unwrap();

    Ok(())
}
