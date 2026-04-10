use std::collections::BTreeMap;
use std::io::{self, Write};

use tera::{Context, Tera};

use crate::models::FileType;
use crate::output_schema::Output;

use super::shared::{io_other, sorted_files};

pub(crate) fn write_html_report(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    let mut license_copyright_rows = Vec::<BTreeMap<String, String>>::new();
    let mut file_rows = Vec::<BTreeMap<String, String>>::new();
    let mut holder_rows = Vec::<BTreeMap<String, String>>::new();
    let mut author_rows = Vec::<BTreeMap<String, String>>::new();
    let mut email_rows = Vec::<BTreeMap<String, String>>::new();
    let mut url_rows = Vec::<BTreeMap<String, String>>::new();
    let mut package_rows = Vec::<BTreeMap<String, String>>::new();

    for file in sorted_files(&output.files) {
        let mut file_row = BTreeMap::new();
        file_row.insert("path".to_string(), file.path.clone());
        file_row.insert(
            "type".to_string(),
            match file.file_type {
                FileType::File => "file",
                FileType::Directory => "directory",
            }
            .to_string(),
        );
        file_row.insert("name".to_string(), file.name.clone());
        file_row.insert("extension".to_string(), file.extension.clone());
        file_row.insert("size".to_string(), file.size.to_string());
        file_row.insert(
            "sha1".to_string(),
            file.sha1.clone().unwrap_or_else(|| "None".to_string()),
        );
        file_row.insert(
            "md5".to_string(),
            file.md5.clone().unwrap_or_else(|| "None".to_string()),
        );
        file_row.insert("files_count".to_string(), String::new());
        file_row.insert("mime_type".to_string(), html_opt(file.mime_type.as_deref()));
        file_row.insert(
            "file_type".to_string(),
            html_opt(file.file_type_label.as_deref()),
        );
        file_row.insert(
            "programming_language".to_string(),
            html_opt(file.programming_language.as_deref()),
        );
        file_row.insert(
            "is_binary".to_string(),
            html_bool(file.is_binary.unwrap_or(false)).to_string(),
        );
        file_row.insert(
            "is_text".to_string(),
            html_bool(file.is_text.unwrap_or(false)).to_string(),
        );
        file_row.insert(
            "is_archive".to_string(),
            html_bool(file.is_archive.unwrap_or(false)).to_string(),
        );
        file_row.insert(
            "is_media".to_string(),
            html_bool(file.is_media.unwrap_or(false)).to_string(),
        );
        file_row.insert(
            "is_source".to_string(),
            html_bool(file.is_source.unwrap_or(false)).to_string(),
        );
        file_row.insert(
            "is_script".to_string(),
            html_bool(file.is_script.unwrap_or(false)).to_string(),
        );
        file_rows.push(file_row);

        for c in &file.copyrights {
            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            row.insert("start".to_string(), c.start_line.to_string());
            row.insert("end".to_string(), c.end_line.to_string());
            row.insert("what".to_string(), "copyright".to_string());
            row.insert("value".to_string(), c.copyright.clone());
            license_copyright_rows.push(row);
        }
        for detection in &file.license_detections {
            for m in &detection.matches {
                let mut row = BTreeMap::new();
                row.insert("path".to_string(), file.path.clone());
                row.insert("start".to_string(), m.start_line.to_string());
                row.insert("end".to_string(), m.end_line.to_string());
                row.insert("what".to_string(), "license".to_string());
                row.insert("value".to_string(), detection.license_expression.clone());
                license_copyright_rows.push(row);
            }
        }

        for h in &file.holders {
            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            row.insert("holder".to_string(), h.holder.clone());
            row.insert("start".to_string(), h.start_line.to_string());
            row.insert("end".to_string(), h.end_line.to_string());
            holder_rows.push(row);
        }
        for a in &file.authors {
            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            row.insert("author".to_string(), a.author.clone());
            row.insert("start".to_string(), a.start_line.to_string());
            row.insert("end".to_string(), a.end_line.to_string());
            author_rows.push(row);
        }
        for e in &file.emails {
            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            row.insert("email".to_string(), e.email.clone());
            row.insert("start".to_string(), e.start_line.to_string());
            row.insert("end".to_string(), e.end_line.to_string());
            email_rows.push(row);
        }
        for u in &file.urls {
            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            row.insert("url".to_string(), u.url.clone());
            row.insert("start".to_string(), u.start_line.to_string());
            row.insert("end".to_string(), u.end_line.to_string());
            url_rows.push(row);
        }

        for package_data in &file.package_data {
            if package_data.package_type.is_none()
                && package_data.name.is_none()
                && package_data.version.is_none()
                && package_data.primary_language.is_none()
            {
                continue;
            }

            let mut row = BTreeMap::new();
            row.insert("path".to_string(), file.path.clone());
            let package_type = serde_json::to_value(package_data.package_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string));
            row.insert(
                "type".to_string(),
                package_type.unwrap_or_else(|| "None".to_string()),
            );
            row.insert(
                "packaging".to_string(),
                html_opt(package_data.namespace.as_deref()),
            );
            row.insert(
                "primary_language".to_string(),
                html_opt(package_data.primary_language.as_deref()),
            );
            package_rows.push(row);
        }
    }

    license_copyright_rows.sort_by(|a, b| {
        a["path"]
            .cmp(&b["path"])
            .then(a["start"].cmp(&b["start"]))
            .then(a["what"].cmp(&b["what"]))
    });
    holder_rows.sort_by(|a, b| a["path"].cmp(&b["path"]).then(a["start"].cmp(&b["start"])));
    author_rows.sort_by(|a, b| a["path"].cmp(&b["path"]).then(a["start"].cmp(&b["start"])));
    email_rows.sort_by(|a, b| a["path"].cmp(&b["path"]).then(a["start"].cmp(&b["start"])));
    url_rows.sort_by(|a, b| a["path"].cmp(&b["path"]).then(a["start"].cmp(&b["start"])));
    package_rows.sort_by(|a, b| a["path"].cmp(&b["path"]).then(a["type"].cmp(&b["type"])));

    let mut context = Context::new();
    context.insert("license_copyright_rows", &license_copyright_rows);
    context.insert("file_rows", &file_rows);
    context.insert("holder_rows", &holder_rows);
    context.insert("author_rows", &author_rows);
    context.insert("email_rows", &email_rows);
    context.insert("url_rows", &url_rows);
    context.insert("package_rows", &package_rows);

    let rendered = Tera::one_off(HTML_REPORT_TEMPLATE, &context, false).map_err(io_other)?;
    writer.write_all(rendered.as_bytes())
}

fn html_opt(value: Option<&str>) -> String {
    value.unwrap_or("None").to_string()
}

fn html_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

const HTML_REPORT_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Custom Template</title>
    <style type="text/css">
      table {
        border-collapse: collapse;
        border: 1px solid gray;
        margin-bottom: 20px;
      }
      td {
        padding: 5px 5px;
        border-style: solid;
        border-width: 1px;
        overflow: hidden;
      }
      th {
        padding: 10px 5px;
        border-style: solid;
        border-width: 1px;
        overflow: hidden;
        border-color: gray;
        color: #fff;
        background-color: #5e81b7;
      }
      tr:nth-child(even) {
        background-color: #ffffff;
      }
      tr:nth-child(odd) {
        background-color: #f9f9f9;
      }
      tr:hover {
        background-color: #eeeeee;
      }
      * {
        font-family: Helvetica, Arial, sans-serif;
        font-weight: normal;
        font-size: 12px;
      }
    </style>
  </head>
  <body>
    <p>Scanned with Provenant</p>

    <table>
      <caption>Copyrights and Licenses Information</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>start</th>
          <th>end</th>
          <th>what</th>
          <th>value</th>
        </tr>
      </thead>
      <tbody>
        {% for row in license_copyright_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.start }}</td>
          <td>{{ row.end }}</td>
          <td>{{ row.what }}</td>
          <td>{{ row.value }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>File Information</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>type</th>
          <th>name</th>
          <th>extension</th>
          <th>size</th>
          <th>sha1</th>
          <th>md5</th>
          <th>files_count</th>
          <th>mime_type</th>
          <th>file_type</th>
          <th>programming_language</th>
          <th>is_binary</th>
          <th>is_text</th>
          <th>is_archive</th>
          <th>is_media</th>
          <th>is_source</th>
          <th>is_script</th>
        </tr>
      </thead>
      <tbody>
        {% for row in file_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.type }}</td>
          <td>{{ row.name }}</td>
          <td>{{ row.extension }}</td>
          <td>{{ row.size }}</td>
          <td>{{ row.sha1 }}</td>
          <td>{{ row.md5 }}</td>
          <td>{{ row.files_count }}</td>
          <td>{{ row.mime_type }}</td>
          <td>{{ row.file_type }}</td>
          <td>{{ row.programming_language }}</td>
          <td>{{ row.is_binary }}</td>
          <td>{{ row.is_text }}</td>
          <td>{{ row.is_archive }}</td>
          <td>{{ row.is_media }}</td>
          <td>{{ row.is_source }}</td>
          <td>{{ row.is_script }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>Holders</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>holder</th>
          <th>start</th>
          <th>end</th>
        </tr>
      </thead>
      <tbody>
        {% for row in holder_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.holder }}</td>
          <td>{{ row.start }}</td>
          <td>{{ row.end }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>Authors</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>Author</th>
          <th>start</th>
          <th>end</th>
        </tr>
      </thead>
      <tbody>
        {% for row in author_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.author }}</td>
          <td>{{ row.start }}</td>
          <td>{{ row.end }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>Emails</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>email</th>
          <th>start</th>
          <th>end</th>
        </tr>
      </thead>
      <tbody>
        {% for row in email_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.email }}</td>
          <td>{{ row.start }}</td>
          <td>{{ row.end }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>Urls</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>url</th>
          <th>start</th>
          <th>end</th>
        </tr>
      </thead>
      <tbody>
        {% for row in url_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.url }}</td>
          <td>{{ row.start }}</td>
          <td>{{ row.end }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>

    <table>
      <caption>Package Information</caption>
      <thead>
        <tr>
          <th>path</th>
          <th>type</th>
          <th>packaging</th>
          <th>primary_language</th>
        </tr>
      </thead>
      <tbody>
        {% for row in package_rows %}
        <tr>
          <td>{{ row.path }}</td>
          <td>{{ row.type }}</td>
          <td>{{ row.packaging }}</td>
          <td>{{ row.primary_language }}</td>
        </tr>
        {% endfor %}
      </tbody>
    </table>
  </body>
  <footer>
    <p>
      Generated with Provenant and provided on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF
      ANY KIND, either express or implied. No content created from Provenant should be considered or
      used as legal advice. Consult an attorney for legal advice. Provenant is a free software code
      scanning tool. Visit
      <a href="https://github.com/mstykow/provenant/">https://github.com/mstykow/provenant/</a>
      for support and download.
    </p>
  </footer>
</html>
"#;
