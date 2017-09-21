use std::io::BufRead;
use std::path::PathBuf;
use std::result::Result as StdResult;
use {time, AtomicDateTime, Run, RunMetadata, Segment, Time, TimeSpan, base64};
use quick_xml::reader::Reader;
use quick_xml::errors::Error as XmlError;
use quick_xml::events::{attributes, BytesStart, Event};
use chrono::{DateTime, ParseError as ChronoError, TimeZone, Utc};
use std::{str, string};
use std::borrow::Cow;
use std::num::{ParseFloatError, ParseIntError};
use std::ops::Deref;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Xml(err: XmlError) {
            from()
        }
        Bool
        UnexpectedEndOfFile
        UnexpectedInnerTag
        AttributeNotFound
        Utf8Str(err: str::Utf8Error) {
            from()
        }
        Utf8String(err: string::FromUtf8Error) {
            from()
        }
        Int(err: ParseIntError) {
            from()
        }
        Float(err: ParseFloatError) {
            from()
        }
        Time(err: time::ParseError) {
            from()
        }
        Date(err: ChronoError) {
            from()
        }
    }
}

pub type Result<T> = StdResult<T, Error>;

#[derive(Copy, Clone, PartialOrd, PartialEq, Ord, Eq)]
struct Version(u32, u32, u32, u32);

struct Tag<'a>(BytesStart<'a>, *mut Vec<u8>);

impl<'a> Deref for Tag<'a> {
    type Target = BytesStart<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> Tag<'a> {
    unsafe fn new(tag: BytesStart<'a>, buf: *mut Vec<u8>) -> Self {
        Tag(tag, buf)
    }

    fn into_buf(self) -> &'a mut Vec<u8> {
        unsafe { &mut *self.1 }
    }
}

struct AttributeValue<'a>(&'a attributes::Attribute<'a>);

impl<'a> AttributeValue<'a> {
    fn get(self) -> Result<Cow<'a, str>> {
        decode_cow_text(self.0.unescaped_value()?)
    }
}

fn decode_cow_text(cow: Cow<[u8]>) -> Result<Cow<str>> {
    Ok(match cow {
        Cow::Borrowed(b) => Cow::Borrowed(str::from_utf8(b)?),
        Cow::Owned(o) => Cow::Owned(String::from_utf8(o)?),
    })
}

fn parse_version<S: AsRef<str>>(version: S) -> Result<Version> {
    let splits = version.as_ref().split('.');
    let mut v = [1, 0, 0, 0];
    for (d, s) in v.iter_mut().zip(splits) {
        *d = s.parse()?;
    }
    Ok(Version(v[0], v[1], v[2], v[3]))
}

fn parse_date_time<S: AsRef<str>>(text: S) -> Result<DateTime<Utc>> {
    Utc.datetime_from_str(text.as_ref(), "%m/%d/%Y %T")
        .map_err(Into::into)
}

fn text_parsed<R, F, T>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(T),
    T: str::FromStr,
    <T as str::FromStr>::Err: Into<Error>,
{
    text_err(reader, buf, |t| {
        f(t.parse().map_err(Into::into)?);
        Ok(())
    })
}

fn text<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(Cow<str>),
{
    text_err(reader, buf, |t| {
        f(t);
        Ok(())
    })
}

fn text_err<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(Cow<str>) -> Result<()>,
{
    loop {
        buf.clear();
        match reader.read_event(buf)? {
            Event::Start(_) => return Err(Error::UnexpectedInnerTag),
            Event::End(_) => {
                f(Cow::Borrowed(""))?;
                return Ok(());
            }
            Event::Text(text) | Event::CData(text) => {
                let text = decode_cow_text(text.unescaped()?)?;
                f(text)?;
                break;
            }
            Event::Eof => return Err(Error::UnexpectedEndOfFile),
            _ => {}
        }
    }
    end_tag_immediately(reader, buf)
}

fn image<R, F>(
    reader: &mut Reader<R>,
    result: &mut Vec<u8>,
    str_buf: &mut String,
    f: F,
) -> Result<()>
where
    R: BufRead,
    F: FnOnce(&str),
{
    text(reader, result, |text| {
        str_buf.clear();
        str_buf.push_str(&text);
    })?;
    result.clear();
    if str_buf.len() >= 216 {
        if let Ok(data) = base64::decode(&str_buf[212..]) {
            result.extend_from_slice(&data[2..data.len() - 1]);
        }
    }
    f(str_buf);
    Ok(())
}

fn time_span<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(TimeSpan),
{
    text_err(reader, buf, |text| {
        let time_span = || -> Result<TimeSpan> {
            if let (Some(dot_index), Some(colon_index)) = (text.find('.'), text.find(':')) {
                if dot_index < colon_index {
                    let days = TimeSpan::from_days(text[..dot_index].parse()?);
                    let time = text[dot_index + 1..].parse()?;
                    return Ok(days + time);
                }
            }
            text.parse().map_err(Into::into)
        }()?;
        f(time_span);
        Ok(())
    })
}

fn time_span_opt<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(Option<TimeSpan>),
{
    text_err(reader, buf, |text| {
        let time_span = || -> Result<Option<TimeSpan>> {
            if text.is_empty() {
                return Ok(None);
            }
            if let (Some(dot_index), Some(colon_index)) = (text.find('.'), text.find(':')) {
                if dot_index < colon_index {
                    let days = TimeSpan::from_days(text[..dot_index].parse()?);
                    let time = text[dot_index + 1..].parse()?;
                    return Ok(Some(days + time));
                }
            }
            Ok(Some(text.parse()?))
        }()?;
        f(time_span);
        Ok(())
    })
}

fn time<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(Time),
{
    let mut time = Time::new();

    parse_children(reader, buf, |reader, tag| if tag.name() == b"RealTime" {
        time_span_opt(reader, tag.into_buf(), |t| { time.real_time = t; })
    } else if tag.name() == b"GameTime" {
        time_span_opt(reader, tag.into_buf(), |t| { time.game_time = t; })
    } else {
        end_tag(reader, tag.into_buf())
    })?;

    f(time);

    Ok(())
}

fn time_old<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, f: F) -> Result<()>
where
    R: BufRead,
    F: FnOnce(Time),
{
    time_span_opt(reader, buf, |t| f(Time::new().with_real_time(t)))
}

fn parse_bool<S: AsRef<str>>(text: S) -> Result<bool> {
    match text.as_ref() {
        "True" => Ok(true),
        "False" => Ok(false),
        _ => Err(Error::Bool),
    }
}

fn end_tag_immediately<R: BufRead>(reader: &mut Reader<R>, buf: &mut Vec<u8>) -> Result<()> {
    loop {
        buf.clear();
        match reader.read_event(buf)? {
            Event::Start(_) => return Err(Error::UnexpectedInnerTag),
            Event::End(_) => return Ok(()),
            Event::Eof => return Err(Error::UnexpectedEndOfFile),
            _ => {}
        }
    }
}

fn end_tag<R: BufRead>(reader: &mut Reader<R>, buf: &mut Vec<u8>) -> Result<()> {
    let mut depth = 0;
    loop {
        buf.clear();
        match reader.read_event(buf)? {
            Event::Start(_) => {
                depth += 1;
            }
            Event::End(_) => {
                if depth == 0 {
                    return Ok(());
                }
                depth -= 1;
            }
            Event::Eof => return Err(Error::UnexpectedEndOfFile),
            _ => {}
        }
    }
}

fn parse_children<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, mut f: F) -> Result<()>
where
    R: BufRead,
    F: FnMut(&mut Reader<R>, Tag) -> Result<()>,
{
    unsafe {
        let ptr_buf: *mut Vec<u8> = buf;
        loop {
            buf.clear();
            match reader.read_event(buf)? {
                Event::Start(start) => {
                    let tag = Tag::new(start, ptr_buf);
                    f(reader, tag)?;
                }
                Event::End(_) => return Ok(()),
                Event::Eof => return Err(Error::UnexpectedEndOfFile),
                _ => {}
            }
        }
    }
}

fn parse_base<R, F>(reader: &mut Reader<R>, buf: &mut Vec<u8>, mut f: F) -> Result<()>
where
    R: BufRead,
    F: FnMut(&mut Reader<R>, Tag) -> Result<()>,
{
    unsafe {
        let ptr_buf: *mut Vec<u8> = buf;
        loop {
            buf.clear();
            match reader.read_event(buf)? {
                Event::Start(start) => {
                    let tag = Tag::new(start, ptr_buf);
                    return f(reader, tag);
                }
                Event::Eof => return Err(Error::UnexpectedEndOfFile),
                _ => {}
            }
        }
    }
}

fn parse_attributes<'a, F>(tag: &BytesStart<'a>, mut f: F) -> Result<()>
where
    F: FnMut(&[u8], AttributeValue) -> Result<bool>,
{
    for attribute in tag.attributes() {
        let attribute = attribute?;
        let key = attribute.key;
        if !f(key, AttributeValue(&attribute))? {
            return Ok(());
        }
    }
    Ok(())
}

fn optional_attribute_err<'a, F>(tag: &BytesStart<'a>, key: &[u8], mut f: F) -> Result<()>
where
    F: FnMut(Cow<str>) -> Result<()>,
{
    parse_attributes(tag, |k, v| if k == key {
        f(v.get()?)?;
        Ok(false)
    } else {
        Ok(true)
    })
}

fn attribute_err<'a, F>(tag: &BytesStart<'a>, key: &[u8], mut f: F) -> Result<()>
where
    F: FnMut(Cow<str>) -> Result<()>,
{
    let mut called = false;
    parse_attributes(tag, |k, v| if k == key {
        f(v.get()?)?;
        called = true;
        Ok(false)
    } else {
        Ok(true)
    })?;
    if called {
        Ok(())
    } else {
        Err(Error::AttributeNotFound)
    }
}

fn attribute<'a, F>(tag: &BytesStart<'a>, key: &[u8], mut f: F) -> Result<()>
where
    F: FnMut(Cow<str>),
{
    attribute_err(tag, key, |t| {
        f(t);
        Ok(())
    })
}

fn parse_metadata<R: BufRead>(
    version: Version,
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    metadata: &mut RunMetadata,
) -> Result<()> {
    if version >= Version(1, 6, 0, 0) {
        parse_children(reader, buf, |reader, tag| if tag.name() == b"Run" {
            attribute(&tag, b"id", |t| metadata.set_run_id(t))?;
            end_tag(reader, tag.into_buf())
        } else if tag.name() == b"Platform" {
            attribute_err(&tag, b"usesEmulator", |t| {
                metadata.set_emulator_usage(parse_bool(t)?);
                Ok(())
            })?;
            text(reader, tag.into_buf(), |t| metadata.set_platform_name(t))
        } else if tag.name() == b"Region" {
            text(reader, tag.into_buf(), |t| metadata.set_region_name(t))
        } else if tag.name() == b"Variables" {
            parse_children(reader, tag.into_buf(), |reader, tag| {
                let mut name = String::new();
                let mut value = String::new();
                attribute(&tag, b"name", |t| { name = t.into_owned(); })?;
                text(reader, tag.into_buf(), |t| { value = t.into_owned(); })?;
                metadata.add_variable(name, value);
                Ok(())
            })
        } else {
            end_tag(reader, tag.into_buf())
        })
    } else {
        end_tag(reader, buf)
    }
}

fn parse_segment<R: BufRead>(
    version: Version,
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    str_buf: &mut String,
    run: &mut Run,
) -> Result<Segment> {
    let mut segment = Segment::new("");

    parse_children(reader, buf, |reader, tag| if tag.name() == b"Name" {
        text(reader, tag.into_buf(), |t| segment.set_name(t))
    } else if tag.name() == b"Icon" {
        image(reader, tag.into_buf(), str_buf, |i| segment.set_icon(i))
    } else if tag.name() == b"SplitTimes" {
        if version >= Version(1, 3, 0, 0) {
            parse_children(reader, tag.into_buf(), |reader, tag| {
                if tag.name() == b"SplitTime" {
                    str_buf.clear();
                    attribute(&tag, b"name", |t| str_buf.push_str(&t))?;
                    run.add_custom_comparison(str_buf.as_str());
                    if version >= Version(1, 4, 1, 0) {
                        time(reader, tag.into_buf(), |t| {
                            *segment.comparison_mut(str_buf) = t;
                        })
                    } else {
                        time_old(reader, tag.into_buf(), |t| {
                            *segment.comparison_mut(str_buf) = t;
                        })
                    }
                } else {
                    end_tag(reader, tag.into_buf())
                }
            })
        } else {
            end_tag(reader, tag.into_buf())
        }
    } else if tag.name() == b"PersonalBestSplitTime" {
        if version < Version(1, 3, 0, 0) {
            time_old(reader, tag.into_buf(), |t| {
                segment.set_personal_best_split_time(t);
            })
        } else {
            end_tag(reader, tag.into_buf())
        }
    } else if tag.name() == b"BestSegmentTime" {
        if version >= Version(1, 4, 1, 0) {
            time(reader, tag.into_buf(), |t| {
                segment.set_best_segment_time(t);
            })
        } else {
            time_old(reader, tag.into_buf(), |t| {
                segment.set_best_segment_time(t);
            })
        }
    } else if tag.name() == b"SegmentHistory" {
        parse_children(reader, tag.into_buf(), |reader, tag| {
            let mut index = 0;
            attribute_err(&tag, b"id", |t| {
                index = t.parse()?;
                Ok(())
            })?;
            if version >= Version(1, 4, 1, 0) {
                time(reader, tag.into_buf(), |t| {
                    segment.segment_history_mut().insert(index, t);
                })
            } else {
                time_old(reader, tag.into_buf(), |t| {
                    segment.segment_history_mut().insert(index, t);
                })
            }
        })
    } else {
        end_tag(reader, tag.into_buf())
    })?;

    Ok(segment)
}

fn parse_run_history<R: BufRead>(
    version: Version,
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    run: &mut Run,
) -> Result<()> {
    if version >= Version(1, 5, 0, 0) {
        end_tag(reader, buf)
    } else if version >= Version(1, 4, 1, 0) {
        parse_children(reader, buf, |reader, tag| {
            let mut index = 0;
            attribute_err(&tag, b"id", |t| {
                index = t.parse()?;
                Ok(())
            })?;
            time(reader, tag.into_buf(), |time| {
                run.add_attempt_with_index(time, index, None, None, None);
            })
        })
    } else {
        parse_children(reader, buf, |reader, tag| {
            let mut index = 0;
            attribute_err(&tag, b"id", |t| {
                index = t.parse()?;
                Ok(())
            })?;
            time_old(reader, tag.into_buf(), |time| {
                run.add_attempt_with_index(time, index, None, None, None);
            })
        })
    }
}

fn parse_attempt_history<R: BufRead>(
    version: Version,
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    run: &mut Run,
) -> Result<()> {
    if version >= Version(1, 5, 0, 0) {
        parse_children(reader, buf, |reader, tag| {
            let mut time = Time::new();
            let mut pause_time = None;
            let mut index = None;
            let (mut started, mut started_synced) = (None, false);
            let (mut ended, mut ended_synced) = (None, false);

            parse_attributes(&tag, |k, v| {
                if k == b"id" {
                    index = Some(v.get()?.parse()?);
                } else if k == b"started" {
                    started = Some(parse_date_time(v.get()?)?);
                } else if k == b"isStartedSynced" {
                    started_synced = parse_bool(v.get()?)?;
                } else if k == b"ended" {
                    ended = Some(parse_date_time(v.get()?)?);
                } else if k == b"isEndedSynced" {
                    ended_synced = parse_bool(v.get()?)?;
                }
                Ok(true)
            })?;

            let index = index.ok_or(Error::AttributeNotFound)?;

            parse_children(reader, tag.into_buf(), |reader, tag| {
                if tag.name() == b"RealTime" {
                    time_span_opt(reader, tag.into_buf(), |t| { time.real_time = t; })
                } else if tag.name() == b"GameTime" {
                    time_span_opt(reader, tag.into_buf(), |t| { time.game_time = t; })
                } else if tag.name() == b"PauseTime" {
                    time_span_opt(reader, tag.into_buf(), |t| { pause_time = t; })
                } else {
                    end_tag(reader, tag.into_buf())
                }
            })?;

            let started = started.map(|t| AtomicDateTime::new(t, started_synced));
            let ended = ended.map(|t| AtomicDateTime::new(t, ended_synced));

            run.add_attempt_with_index(time, index, started, ended, pause_time);

            Ok(())
        })
    } else {
        end_tag(reader, buf)
    }
}

pub fn parse<R: BufRead>(source: R, path: Option<PathBuf>) -> Result<Run> {
    let reader = &mut Reader::from_reader(source);
    reader.expand_empty_elements(true);
    reader.trim_text(true);

    let mut buf = Vec::with_capacity(4096);
    let mut str_buf = String::with_capacity(4096);

    let mut run = Run::new();

    parse_base(reader, &mut buf, |reader, tag| {
        if tag.name() == b"Run" {
            let mut version = Version(1, 0, 0, 0);
            optional_attribute_err(&tag, b"version", |t| {
                version = parse_version(t)?;
                Ok(())
            })?;

            parse_children(reader, tag.into_buf(), |reader, tag| {
                if tag.name() == b"GameIcon" {
                    image(
                        reader,
                        tag.into_buf(),
                        &mut str_buf,
                        |i| run.set_game_icon(i),
                    )
                } else if tag.name() == b"GameName" {
                    text(reader, tag.into_buf(), |t| run.set_game_name(t))
                } else if tag.name() == b"CategoryName" {
                    text(reader, tag.into_buf(), |t| run.set_category_name(t))
                } else if tag.name() == b"Offset" {
                    time_span(reader, tag.into_buf(), |t| run.set_offset(t))
                } else if tag.name() == b"AttemptCount" {
                    text_parsed(reader, tag.into_buf(), |t| run.set_attempt_count(t))
                } else if tag.name() == b"AttemptHistory" {
                    parse_attempt_history(version, reader, tag.into_buf(), &mut run)
                } else if tag.name() == b"RunHistory" {
                    parse_run_history(version, reader, tag.into_buf(), &mut run)
                } else if tag.name() == b"Metadata" {
                    parse_metadata(version, reader, tag.into_buf(), run.metadata_mut())
                } else if tag.name() == b"Segments" {
                    parse_children(reader, tag.into_buf(), |reader, tag| {
                        if tag.name() == b"Segment" {
                            let segment = parse_segment(
                                version,
                                reader,
                                tag.into_buf(),
                                &mut str_buf,
                                &mut run,
                            )?;
                            run.push_segment(segment);
                            Ok(())
                        } else {
                            end_tag(reader, tag.into_buf())
                        }
                    })
                } else if tag.name() == b"AutoSplitterSettings" {
                    // TODO Store this somehow
                    end_tag(reader, tag.into_buf())
                } else {
                    end_tag(reader, tag.into_buf())
                }
            })?;
        }
        Ok(())
    })?;

    run.set_path(path);

    Ok(run)
}
