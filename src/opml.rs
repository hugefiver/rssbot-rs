use std::io::Cursor;
use std::io::Write;

use chrono::Local;
use quick_xml::Writer;
use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use crate::data::FeedInfo;

pub fn into_opml(feeds: Vec<FeedInfo>) -> String {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let decl = BytesDecl::new("1.0", Some("UTF-8"), None);
    writer.write_event(Event::Decl(decl)).unwrap();

    with_tag(
        &mut writer,
        "opml",
        &mut [Attribute::from(("version", "2.0")).into()],
        |writer| {
            with_tag(writer, "head", &mut [], |writer| {
                with_tag(writer, "title", &mut [], |writer| {
                    let text = BytesText::new("Exported from RSSBot");
                    writer.write_event(Event::Text(text))?;
                    Ok(())
                })?;
                with_tag(writer, "dateCreated", &mut [], |writer| {
                    // e.g. Thu, 02 Nov 2017 18:08:24 CST
                    let time = Local::now().format("%a, %d %b %Y %T %Z").to_string();
                    let text = BytesText::new(&time);
                    writer.write_event(Event::Text(text))?;
                    Ok(())
                })?;
                with_tag(writer, "docs", &mut [], |writer| {
                    let text = BytesText::new("http://www.opml.org/spec2");
                    writer.write_event(Event::Text(text))?;
                    Ok(())
                })
            })?;
            with_tag(writer, "body", &mut [], move |writer| {
                for feed in feeds {
                    let mut outline = BytesStart::new("outline");
                    outline.push_attribute(Attribute::from(("type", "rss")));
                    outline.push_attribute(Attribute::from(("text", feed.title.as_str())));
                    outline.push_attribute(Attribute::from(("xmlUrl", feed.link.as_str())));
                    writer.write_event(Event::Empty(outline))?;
                }
                Ok(())
            })
        },
    )
    .unwrap();

    String::from_utf8(writer.into_inner().into_inner())
        .expect("quick-xml always produces valid UTF-8")
}

// type of `attrs` is for zero allocation
fn with_tag<W, F>(
    writer: &mut Writer<W>,
    tag: &str,
    attrs: &mut [Option<Attribute<'_>>],
    then: F,
) -> quick_xml::Result<()>
where
    W: Write,
    F: FnOnce(&mut Writer<W>) -> quick_xml::Result<()>,
{
    let mut start = BytesStart::new(tag);
    for attr in attrs.iter_mut() {
        start.push_attribute(attr.take().unwrap());
    }
    writer.write_event(Event::Start(start)).unwrap();
    then(writer)?;
    let end = BytesEnd::new(tag);
    writer.write_event(Event::End(end)).unwrap();
    Ok(())
}

#[test]
fn test_to_opml() {
    let feed1 = FeedInfo {
        title: "title1".into(),
        link: "link1".into(),
    };
    let feed2 = FeedInfo {
        title: "title2".into(),
        link: "link2".into(),
    };
    let feeds = vec![feed1, feed2];
    let r = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <opml version=\"2.0\">\
         <head>\
         <title>Exported from RSSBot</title>\
         <dateCreated>{}</dateCreated>\
         <docs>http://www.opml.org/spec2</docs>\
         </head>\
         <body>\
         <outline type=\"rss\" text=\"title1\" xmlUrl=\"link1\"/>\
         <outline type=\"rss\" text=\"title2\" xmlUrl=\"link2\"/>\
         </body>\
         </opml>",
        Local::now().format("%a, %d %b %Y %T %Z")
    );
    assert_eq!(into_opml(feeds), r);
}
