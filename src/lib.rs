mod utils;

use wasm_bindgen::prelude::*;
use thiserror::Error;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

const base_url: &'static str = "https://york.hackspace.org.uk/mediawiki/api.php";

#[derive(Error,Debug)]
enum ApiError{
    #[error("Request failed: {}", .0)]
    Req(#[from] reqwest_wasm::Error),
    #[error("Failed to decode reply as JSON: {}", .0)]
    Parse(#[from] serde_json::Error),
}

async fn api_request(args: &str) -> Result<serde_json::Value, ApiError> {
    let origin = web_sys::window().unwrap().origin();
    let url = format!("{}?origin={}&{}", base_url.to_owned(), urlencoding::encode(&origin), args);
    let res_text = reqwest_wasm::get(url).await?.text().await?;
    let parsed = serde_json::from_str(&res_text)?;
    Ok(parsed)
}

#[wasm_bindgen]
pub async fn get_names() -> Result<js_sys::Array, JsValue>{
    let mut result = js_sys::Array::new();
    const limit:usize = 999; // if it takes more than 1000 requests, probably something wrong
    let base_req = "action=query&format=json&generator=allpages&gapprefix=Equipment%2F";
    let mut cont = "".to_string();
    let mut did_complete = false;
    for i in 0..limit{
        let mdata = api_request(&format!("{}{}", base_req, cont)).await;
        let data = match mdata{
            Ok(a) => a,
            Err(e) => return Err(format!("Error fetching data: {}", e).into())
        };

        // add new data to list
        let q = &data["query"];
        if let serde_json::Value::Null = q { return Err("Query data missing from JSON response".into()); }
        let pages = &q["pages"];
        if let serde_json::Value::Object(page_map) = pages {
            for page in page_map.values(){
                if let serde_json::Value::String(title) = &page["title"] {
                    if title.len() > "Equipment/".len(){
                        result.push(&JsValue::from_str(&title));
                    }
                }
                else{
                    return Err("page title missing from page in JSON response".into());
                }
            }
        }
        else {
            return Err("pages missing from query in JSON response".into());
        }

        // Check for completion
        if let serde_json::Value::Null = &data["continue"]{
            did_complete = true;
            break;
        }

        if let serde_json::Value::String(s) = &data["continue"]["gapcontinue"]{
            cont = format!("&gapcontinue={}", urlencoding::encode(&s));
        }
        else{
            return Err("Continue name missing, api broken?".into());
        }
    }
    if did_complete {
        Ok(result)
    }
    else{
        Err(format!("Too many requests, page listing incomplete after {} requests.", limit).into())
    }
}
#[wasm_bindgen]
pub async fn get_props(name: String) -> Result<String, JsValue>{
    let url = format!("action=parse&format=json&page={}&prop=parsetree&contentmodel=wikitext", name);
    let mdata = api_request(&url).await;
    let data = match mdata{
        Ok(a) => a,
        Err(e) => return Err(format!("Error fetching data: {}", e).into())
    };

    console_log!("{}", data);
    Ok("TODO".to_string())
}
