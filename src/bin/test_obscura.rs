#[tokio::main]
async fn main() {
    match agents_rust::tools::obscura_browser::get_browser().await {
        Ok(browser) => {
            println!("1. ObscuraBrowser created OK");

            match browser.navigate("https://example.com").await {
                Ok(text) => {
                    println!("2. Navigate OK");
                    println!("   Text preview: {}", &text[..text.len().min(100)]);
                }
                Err(e) => println!("2. Navigate FAIL: {}", e),
            }

            match browser.evaluate_js("document.title").await {
                Ok(title) => println!("3. JS eval OK: title='{}'", title),
                Err(e) => println!("3. JS eval FAIL: {}", e),
            }

            println!("ALL DONE");
        }
        Err(e) => println!("FAIL to create ObscuraBrowser: {}", e),
    }
}
