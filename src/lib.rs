#![allow(non_upper_case_globals)]
mod utils;

use wasm_bindgen::prelude::*;
use thiserror::Error;
use std::collections::HashMap;
use base64::Engine;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

const base_url: &'static str = "https://york.hackspace.org.uk/mediawiki/api.php";
const id_base_url: &'static str = "HTTPS://YHS.MOD3.UK/W/";
const image_thumb_url: &'static str = "https://york.hackspace.org.uk/mediawiki/thumb.php?w=400&f=";
const template_112x45mm: &'static str = include_str!("../template_112x45mm.svg");
const template_96x34mm: &'static str = include_str!("../template_96x34mm.svg");
const template_45x45mm: &'static str = include_str!("../template_45x45mm.svg");

fn esc_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out+="&lt;",
            '>' => out+="&gt;",
            '"' => out+="&#34;",
            '\'' => out+="&#39;",
            '&' => out+="&#38;",
            c => out+=&c.to_string(),
        }
    }
    out
}

fn expand_template(template: &str, vars: HashMap<String, String>) -> String{
    let mut result = template.to_string();
    for (key, value) in vars{
        if key.starts_with("NOESC"){
            result = result.replace(&format!("{{{{{}}}}}", key[5..].to_string()), &value);
        }
        else{
            result = result.replace(&format!("{{{{{}}}}}", key), &esc_xml(&value));
        }
    }
    result
}

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
    for _ in 0..limit{
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

fn unwrap_name(s: &str) -> String{
    if s.starts_with("[[User:") && s.ends_with("]]") {
        s[7..s.len()-2].to_string()
    }
    else{
        s.to_string()
    }
}

async fn gen_one_sticker(name: &str) -> Result<String, JsValue>{
    let url = format!("action=parse&format=json&page={}&prop=parsetree&contentmodel=wikitext", urlencoding::encode(name));
    let mdata = api_request(&url).await;
    let data = match mdata{
        Ok(a) => a,
        Err(e) => return Err(format!("Error fetching data: {}", e).into())
    };

    let xml = &data["parse"]["parsetree"]["*"];
    let id_num = &data["parse"]["pageid"].as_u64().ok_or_else(||JsValue::from_str("Failed to parse page ID from API response"))?;
    if let serde_json::Value::String(xmltext) = xml{
        use xmltree::Element;
        let root = Element::parse(xmltext.as_bytes());
        if let Err(a) = root{
            return Err(format!("{:?}",a).into());
        }
        let root = root.unwrap();
        let result = (||->Option<HashMap<_,_>>{
            let template = root.get_child("template")?;
            let tname = template.get_child("title")?.get_text()?;
            if tname.trim() == "EquipmentInfobox"{
                let url = format!("{}{}", id_base_url, id_num);
                let mut info = HashMap::<_,_>::new();
                info.insert("url".to_string(), url);
                use xmltree::XMLNode as XN;
                for child in &template.children{
                    match child{
                        XN::Element(e) => {
                            if e.name == "part"{
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
        if let Some(mut info) = result{
            let template = info.get("sticker_sz").map(String::to_owned).unwrap_or_else(||"112x45mm".to_string());
            let owner = info.get("owner").map(String::to_owned);
            let url = info.get("url").map(String::to_owned);
            let img_name = info.get("image").map(String::to_owned);
            let training = info.get("trainingurl").map(String::to_owned).or_else(||{
                info.get("trainingform").map(String::to_owned)
            });
            let req_training = training.is_some();
            info.insert("training".to_string(), if req_training {"DO NOT USE without training!".to_string()} else {"no training required".to_string()});
            info.insert("bgstyle".to_string(), if req_training {"fill:#ff6b72;fill-opacity:1;".to_string()} else {"fill:#ffffff;fill-opacity:1;".to_string()});
            let mut is_ltl = true;
            let lcowner = owner.clone().unwrap_or("".to_string()).to_lowercase();
            for s in ["york hackspace", "hackspace", "york hack space", "hack space", "yhs", ""]{
                if lcowner == s{
                    is_ltl = false;
                }
            }
            if !is_ltl{
                info.insert("owner".to_string(), "Owned by York Hackspace".to_string());
            }
            else{
                info.insert("owner".to_string(), format!("Kindly loaned by {}", unwrap_name(&owner.as_ref().unwrap())));
            }
            let qr = qrcode_generator::to_svg_to_string(&url.as_ref().unwrap(), qrcode_generator::QrCodeEcc::Low, 200, None::<&str>).unwrap();
            info.insert("NOESCqrcode_svg".to_string(), qr);
            let has_web_interface = info.contains_key("webinterface");
            if !has_web_interface{
                info.insert("webinterface".to_string(), "".to_string());
            }
            if img_name.is_some() && !has_web_interface{
                let image = img_name.unwrap();
                let url = format!("{}{}", image_thumb_url, urlencoding::encode(&image));
                let client = reqwest_wasm::Client::builder().build()?;
                let req = client.get(url).header(reqwest_wasm::header::ACCEPT, "image/*");
                let resp = req.send().await?;
                let mime = resp.headers().get("Content-Type").map_or("image/jpeg", |a|reqwest_wasm::header::HeaderValue::to_str(a).unwrap()).to_string();
                let img_data = resp.bytes().await?;
                let data = base64::engine::general_purpose::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::GeneralPurposeConfig::default()).encode(img_data);
                info.insert("NOESCimage".to_string(), format!("<image href=\"data:{};base64,{}\" x=\"35\" y=\"20\" width=\"65\" height=\"20\" />", mime, data));
            }
            info.remove("image");
            let template = match template.as_ref() {
                "112x45mm" => template_112x45mm,
                "96x34mm" => template_96x34mm,
                "45x45mm" => template_45x45mm,
                s => return Err(format!("Sticker size is set to {}, but no template for this size exists.", s).into()),
            };
            Ok(expand_template(template, info))
        }
        else{
            Err("Failed to find and parse EquipmentInfobox on page. Does it exist?".into())
        }
    }
    else{
        Err("Internal error: API response does not include parse tree for requested page".into())
    }
}

fn generate_download_link(svgs: &mut dyn std::iter::Iterator<Item=(&str, &str)>) -> String {
    use std::io::Write;
    let buf = Vec::<u8>::new();
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(buf));
    for (name, data) in svgs{
        zip.start_file(&format!("{}.svg",name), options).unwrap();
        zip.write(data.as_bytes()).unwrap();
    }
    let buf = zip.finish().unwrap().into_inner();
    format!("data:application/zip;base64,{}", base64::engine::general_purpose::GeneralPurpose::new(&base64::alphabet::STANDARD, base64::engine::general_purpose::GeneralPurposeConfig::default()).encode(buf))
}

#[wasm_bindgen]
pub async fn gen_stickers(names: String) -> Result<js_sys::Array, JsValue> {
    utils::set_panic_hook();
    let mut results = Vec::new();
    let mut errlog = String::new();
    let names = names.trim();
    for name in names.split("\n"){
        console_log!("Generating sticker for: {}", name);
        let r = gen_one_sticker(name).await;
        match r{
            Ok(sticker) => results.push(sticker),
            Err(e) => errlog += &format!("Failed to generate sticker for \"{}\": {}\n", name, e.as_string().unwrap_or_else(||{format!("{:?}", e)})),
        }
    }
    let res = js_sys::Array::new();
    res.push(&errlog.into());
    res.push(&results.join("").into());
    let dl_link = generate_download_link(
        &mut names.split("\n").zip(
            (results.iter()).map(String::as_str)
        )
    );
    res.push(&dl_link.into());
    Ok(res)
}
