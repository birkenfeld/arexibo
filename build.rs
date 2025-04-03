// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Build script that processes the XMDS WSDL file into Rust code
//! that can call the services using ureq.

use std::{fs::File, io::Write, path::PathBuf};
use elementtree::Element;

const WSDLFILE: &str = "xmds_v5.wsdl";

const MESSAGE: &str = "{http://schemas.xmlsoap.org/wsdl/}message";
const PORT_TYPE: &str = "{http://schemas.xmlsoap.org/wsdl/}portType";
const OPERATION: &str = "{http://schemas.xmlsoap.org/wsdl/}operation";

const HEADER: &str = "// Auto-generated by build.rs.

use std::{fmt, str::FromStr};
use anyhow::{bail, Context, Result};
use elementtree::Element;
use ureq::Agent;
use crate::util::Base64Field;
";

const SERVICE_IMPL: &str = r###"pub struct Service {
    baseuri: String,
    agent: Agent,
}

impl Service {
    pub fn new(baseuri: String, agent: Agent) -> Self {
        Self { baseuri, agent }
    }

    fn request<T: FromStr<Err = anyhow::Error> + fmt::Debug>(&mut self, name: &str, body: impl fmt::Display) -> Result<T>
    {
        log::debug!("calling XMDS {}", name);
        let data = format!(r#"
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
               xmlns:soapenc="http://schemas.xmlsoap.org/soap/encoding/"
               xmlns:tns="urn:xmds" xmlns:types="urn:xmds/encodedTypes"
               xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xmlns:xsd="http://www.w3.org/2001/XMLSchema">
<soap:Body soap:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<tns:{name}>
{body}
</tns:{name}>
</soap:Body>
</soap:Envelope>"#);
        self.agent
            .post(&self.baseuri)
            .config().http_status_as_error(false).build()
            .send(&data)
            .with_context(|| format!("sending {} SOAP request", name))?
            .into_body().read_to_string().with_context(|| format!("decoding {} SOAP response", name))?
            .parse().with_context(|| format!("parsing {} SOAP response", name))
    }
"###;

fn main() {
    build_qtlib();
    convert_wsdl();
}

fn build_qtlib() {
    let dst = cmake::build("gui");
    println!("cargo:rerun-if-changed=gui/lib.cpp");
    println!("cargo:rerun-if-changed=gui/lib.h");
    println!("cargo:rerun-if-changed=gui/view.cpp");
    println!("cargo:rerun-if-changed=gui/view.h");
    println!("cargo:rustc-link-search=native={}/build", dst.display());
    println!("cargo:rustc-link-lib=static=arexibogui");

    let linker_script = std::fs::read_to_string(
        format!("{}/build/CMakeFiles/dummy.dir/link.txt", dst.display())).unwrap();
    for line in linker_script.lines() {
        if line.contains(" -lc ") {
            let libpart = line.split(" -lc ").nth(1).unwrap();
            let libs = shlex::split(libpart).unwrap();
            for lib in libs {
                println!("cargo:rustc-link-arg={}", lib);
            }
        }
    }
}

fn convert_wsdl() {
    println!("cargo:rerun-if-changed={}", WSDLFILE);
    let tree = Element::from_reader(File::open(WSDLFILE).unwrap()).unwrap();

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut out = File::create(out_dir.join("xmds_soap.rs")).unwrap();

    writeln!(out, "{}", HEADER).unwrap();

    // Go through all messages
    for msg in tree.find_all(MESSAGE) {
        let name = msg.get_attr("name").unwrap().to_string();
        let is_req = name.ends_with("Request");
        let mut msg_members = vec![];

        // Collect members of the structure
        for part in msg.children() {
            let pname = part.get_attr("name").unwrap().to_string();
            let rsname = if pname == "type" { "r#type" } else { &*pname }.to_string();
            let xsdtype = part.get_attr("type").unwrap().to_string();
            let rstype = match &*xsdtype {
                "xsd:string" => if is_req { "&'a str" } else { "String" },
                "xsd:int" => "i64",
                "xsd:double" => "f64",
                "xsd:base64Binary" => "Base64Field",
                "xsd:boolean" => "bool",
                _ => unimplemented!()
            };
            msg_members.push((pname, rsname, xsdtype, rstype));
        }

        // Write struct definition
        writeln!(out, "#[derive(Debug)] pub struct {}{} {{",
                 name, if is_req { "<'a>" } else { "" }).unwrap();
        for (_, rsname, _, rstype) in &msg_members {
            writeln!(out, "    pub {}: {},", rsname, rstype).unwrap();
        }
        writeln!(out, "}}\n").unwrap();

        if is_req {
            // Write serialization code if it's a request type
            writeln!(out, r#"impl<'a> fmt::Display for {}<'a> {{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{"#, name).unwrap();

            for (pname, rsname, xsdtype, _) in &msg_members {
                writeln!(out, r#"        write!(f, "<{} xsi:type=\"{}\">{{}}</{}>", self.{})?;"#,
                       pname, xsdtype, pname, rsname).unwrap();
            }

            writeln!(out, r#"        Ok(())
    }}
}}
"#).unwrap();
        } else {
            // Write deserialization code if it's a response type
            writeln!(out, r#"impl FromStr for {} {{
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {{
        let tree = Element::from_reader(&mut s.as_bytes()).context("XML parse")?;
        let tns = tree.get_child(0).and_then(|c| c.get_child(0))
                      .context("missing SOAP envelope")?;
        if tns.tag().name() != "{}" {{
            if tns.tag().name() == "Fault" {{
                bail!("got SOAP fault: {{}}", tns.find("faultstring")
                                                 .map_or("no fault string", |fs| fs.text()));
            }} else {{
                bail!("got unexpected content tag: {{}}", tns.tag().name());
            }}
        }}
        Ok(Self {{"#, name, name).unwrap();
            for (pname, rsname, _, _) in &msg_members {
                writeln!(out, r#"            {}: tns.find("{}").context("missing {}")?.text().parse().context("parsing {}")?,"#,
                       rsname, pname, pname, pname).unwrap();
            }
            writeln!(out, "        }})
    }}
}}\n").unwrap();
        }
    }

    // Write Service impl with methods for each operation
    writeln!(out, "{}", SERVICE_IMPL).unwrap();

    for ptype in tree.find_all(PORT_TYPE) {
        for port in ptype.find_all(OPERATION) {
            let name = port.get_attr("name").unwrap();
            let inp = port.get_child(1).unwrap()
                          .get_attr("message").unwrap()
                          .trim_start_matches("tns:");
            let outp = port.get_child(2).unwrap()
                           .get_attr("message").unwrap()
                           .trim_start_matches("tns:");

            writeln!(out, "    pub fn {}(&mut self, arg: {}) -> Result<{}> {{ self.request(\"{}\", arg) }}\n",
                   name, inp, outp, name).unwrap();
        }
    }

    writeln!(out, "}}").unwrap();
}
