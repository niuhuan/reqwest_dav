use crate::{Auth, ClientBuilder, Depth};

#[tokio::test]
async fn it_works() -> crate::Result<()> {
    let client = ClientBuilder::new()
        .set_host("http://sever/remote.php/dav/files/user/".to_string())
        .set_auth(Auth::Basic("username".to_owned(), "password".to_owned()))
        .build()?;

    // let response = client.list("", Depth::Infinity).await?; // 207
    // let _ = client.mkcol("test").await?; // 201
    // let _ = client.put("test/1.txt", "123\n").await?; // 201
    // let _ = client.mv("test/1.txt", "test/2.txt").await?; // 201
    // let _ = client.get("test/2.txt").await?; // 200
    // let _ = client.delete("test").await?; // 204
    // println!("STATUS : {}", response.status().as_str());
    // println!("BODY : {}", response.text().await?);

    println!(
        "{}",
        serde_json::to_string(&client.list_entities("", Depth::Infinity).await?).unwrap()
    );

    Ok(())
}
