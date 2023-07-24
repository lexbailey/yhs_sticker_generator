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
const page_base_url: &'static str = "https://york.hackspace.org.uk/wiki/";

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
pub async fn get_names() -> Result<String, JsValue>{
    let mut result = Vec::new();
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
                        result.push(title.to_string());
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
        Ok(result.join("\n"))
    }
    else{
        Err(format!("Too many requests, page listing incomplete after {} requests.", limit).into())
    }
}
use std::collections::HashMap;

async fn gen_one_sticker(name: &str) -> Result<String, JsValue>{
    let url = format!("action=parse&format=json&page={}&prop=parsetree&contentmodel=wikitext", name);
    let mdata = api_request(&url).await;
    let data = match mdata{
        Ok(a) => a,
        Err(e) => return Err(format!("Error fetching data: {}", e).into())
    };

    console_log!("{}", data);

    let xml = &data["parse"]["parsetree"]["*"];
    if let serde_json::Value::String(xmltext) = xml{
        use xmltree::Element;
        let root = Element::parse(xmltext.as_bytes());
        if let Err(a) = root{
            return Err(format!("{:?}",a).into());
        }
        let root = root.unwrap();
        let result = (||->Option<HashMap<_,_>>{
            let mut template = root.get_child("template")?;
            let mut tname = template.get_child("title")?.get_text()?;
            if tname.trim() == "EquipmentInfobox"{
                let mut url = format!("{}{}", page_base_url, name);
                let mut info = HashMap::<_,_>::new();
                info.insert("url".to_string(), url);
                use xmltree::XMLNode as XN;
                for child in &template.children{
                    match child{
                        XN::Element(e) => {
                            if e.name == "part"{
                                console_log!("a part! {:?}", e);
                                let key = e.get_child("name")?.get_text()?;
                                let value = e.get_child("value")?.get_text().unwrap_or("".to_string().into());
                                info.insert(key.trim().to_string(), value.trim().to_string());
                            }
                        },
                        _ => {},
                    }
                }
                Some(info)
            }
            else{
                None
            }
        })();
        if let Some(info) = result{
            let name = info.get("name").map(String::to_owned);
            let owner = info.get("owner").map(String::to_owned);
            let url = info.get("url").map(String::to_owned);
            let img_name = info.get("image").map(String::to_owned);
            let training = info.get("trainingurl").map(String::to_owned).or_else(||{
                info.get("trainingform").map(String::to_owned)
            });
            let req_training = training.is_some();
            let mut is_ltl = true;
            let lcowner = owner.clone().unwrap_or("".to_string()).to_lowercase();
            for s in ["york hackspace", "hackspace", "york hack space", "hack space", "yhs", ""]{
                if lcowner == s{
                    is_ltl = false;
                }
            }
            console_log!("{:?}", info);
            console_log!("Name: {:?}", name);
            console_log!("URL: {:?}", url);
            console_log!("Image: {:?}", img_name);
            console_log!("Requires training? {:?}", req_training);
            console_log!("Is long term loan? {:?}", is_ltl);
            console_log!("Owner: {:?}", owner);
            Ok("<svg></svg>".to_string())
        }
        else{
            Err("Failed to parse EquipmentInfobox on page".into())
        }
    }
    else{
        Err("API response does not include parse tree for requested page".into())
    }
}

#[wasm_bindgen]
pub async fn gen_stickers(names: String) -> Result<String, JsValue> {
    console_log!("Raw list: {:?}", names);
    let mut results = Vec::new();
    for name in names.trim().split("\n"){
        console_log!("Generating sticker for: {}", name);
        results.push(gen_one_sticker(name).await?);
    }
    Ok(results.join(""))
}
