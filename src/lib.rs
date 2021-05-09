mod apkg_col;
mod apkg_schema;
mod builders;
mod builtin_models;
mod card;
mod db_entries;
mod deck;
mod model;
mod note;
mod package;
mod util;

pub use builders::{Field, Template};
pub use builtin_models::*;
pub use deck::Deck;
pub use model::Model;
pub use note::Note;
pub use package::Package;

#[cfg(test)]
mod tests {
    use super::*;
    use db_entries::Req;
    use pyo3::types::PyDict;
    use pyo3::{
        types::{PyModule, PyString},
        PyAny, Python,
    };
    use serial_test::serial;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir, TempPath};

    fn model() -> Model {
        Model::new(
            234567,
            "foomodel",
            vec![Field::new("AField"), Field::new("BField")],
            vec![Template::new("card1")
                .qfmt("{{AField}}")
                .afmt(r#"{{FrontSide}}<hr id="answer">{{BField}}"#)],
        )
    }

    fn cn_model() -> Model {
        Model::new(
            345678,
            "Chinese",
            vec![
                Field::new("Traditional"),
                Field::new("Simplified"),
                Field::new("English"),
            ],
            vec![
                Template::new("Traditional")
                    .qfmt("{{Traditional}}")
                    .afmt(r#"{{FrontSide}}<hr id="answer">{{English}}"#),
                Template::new("Simplified")
                    .qfmt("{{Simplified}}")
                    .afmt(r#"{{FrontSide}}<hr id="answer">{{English}}"#),
            ],
        )
    }

    fn model_with_hint() -> Model {
        Model::new(
            456789,
            "with hint",
            vec![
                Field::new("Question"),
                Field::new("Hint"),
                Field::new("Answer"),
            ],
            vec![Template::new("card1")
                .qfmt("{{Question}}{{#Hint}}<br>Hint: {{Hint}}{{/Hint}}")
                .afmt("{{Answer}}")],
        )
    }

    const CUSTOM_LATEX_PRE: &str = r#"\documentclass[12pt]{article}
    \special{papersize=3in,5in}
    \usepackage[utf8]{inputenc}
    \usepackage{amssymb,amsmath,amsfonts}
    \pagestyle{empty}
    \setlength{\parindent}{0in}
    \begin{document}
    "#;

    const CUSTOM_LATEX_POST: &str = "% here is a great comment\n\\end{document}";

    fn model_with_latex() -> Model {
        Model::new_with_options(
            567890,
            "with latex",
            vec![Field::new("AField"), Field::new("Bfield")],
            vec![Template::new("card1")
                .qfmt("{{AField}}")
                .afmt(r#"{{FrontSide}}<hr id="answer">{{BField}}"#)],
            None,
            None,
            Some(CUSTOM_LATEX_PRE),
            Some(CUSTOM_LATEX_POST),
            None,
        )
    }

    const CUSTOM_SORT_FIELD_INDEX: i64 = 1;

    fn model_with_sort_field_index() -> Model {
        Model::new_with_options(
            567890,
            "with latex",
            vec![Field::new("AField"), Field::new("Bfield")],
            vec![Template::new("card1")
                .qfmt("{{AField}}")
                .afmt(r#"{{FrontSide}}<hr id="answer">{{BField}}"#)],
            None,
            None,
            None,
            None,
            Some(CUSTOM_SORT_FIELD_INDEX),
        )
    }

    const VALID_MP3: &[u8] =
        b"\xff\xe3\x18\xc4\x00\x00\x00\x03H\x00\x00\x00\x00LAME3.98.2\x00\x00\x00\
        \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
        \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
        \x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

    const VALID_JPG: &[u8] =
        b"\xff\xd8\xff\xdb\x00C\x00\x03\x02\x02\x02\x02\x02\x03\x02\x02\x02\x03\x03\
        \x03\x03\x04\x06\x04\x04\x04\x04\x04\x08\x06\x06\x05\x06\t\x08\n\n\t\x08\t\
        \t\n\x0c\x0f\x0c\n\x0b\x0e\x0b\t\t\r\x11\r\x0e\x0f\x10\x10\x11\x10\n\x0c\
        \x12\x13\x12\x10\x13\x0f\x10\x10\x10\xff\xc9\x00\x0b\x08\x00\x01\x00\x01\
        \x01\x01\x11\x00\xff\xcc\x00\x06\x00\x10\x10\x05\xff\xda\x00\x08\x01\x01\
        \x00\x00?\x00\xd2\xcf \xff\xd9";

    pub fn anki_collection<'a>(py: &'a Python, col_fname: &str) -> &'a PyAny {
        let code = r#"
import anki
import tempfile

def setup(fname):
    import uuid
    colf_name = f"{fname}.anki2"
    return anki.Collection(colf_name)
"#;
        let setup = PyModule::from_code(*py, code, "test_setup", "test_setup.py")
            .unwrap()
            .to_owned();
        let col = setup
            .call1("setup", (PyString::new(*py, col_fname),))
            .unwrap();
        col
    }

    struct TestSetup<'a> {
        py: &'a Python<'a>,
        col: &'a PyAny,
        col_fname: String,
        tmp_files: Vec<TempPath>,
        tmp_dirs: Vec<TempDir>,
    }

    impl<'a> Drop for TestSetup<'a> {
        fn drop(&mut self) {
            let code = r#"
import os
import time
import shutil
def cleanup(fname, col):
    col.close()
    path = col.path
    media = path.split(".anki2")[0] + '.media'
    os.remove(path)
    shutil.rmtree(media)
                "#;
            let cleanup = PyModule::from_code(*self.py, code, "test_cleanup", "test_cleanup.py")
                .unwrap()
                .to_owned();
            cleanup
                .call(
                    "cleanup",
                    (PyString::new(*self.py, &self.col_fname), self.col),
                    None,
                )
                .unwrap();
        }
    }

    impl<'a> TestSetup<'a> {
        pub fn new(py: &'a Python<'a>) -> Self {
            let mut tmp_dirs = vec![];
            let curr = if let Ok(curr) = std::env::current_dir() {
                curr
            } else {
                let tmp_dir = TempDir::new().unwrap();
                std::env::set_current_dir(tmp_dir.path()).unwrap();
                tmp_dirs.push(tmp_dir);
                std::env::current_dir().unwrap()
            };
            let col_fname = uuid::Uuid::new_v4().to_string();
            let col = anki_collection(py, &col_fname);
            std::env::set_current_dir(curr).unwrap();
            Self {
                py,
                col,
                col_fname,
                tmp_files: vec![],
                tmp_dirs,
            }
        }

        pub fn import_package(&mut self, mut package: Package, timestamp: Option<f64>) {
            self.tmp_files
                .push(NamedTempFile::new().unwrap().into_temp_path());
            let out_file = self.tmp_files.last().unwrap();
            if let Some(ts) = timestamp {
                package
                    .write_to_file_timestamp(out_file.to_str().unwrap(), ts)
                    .unwrap();
            } else {
                package.write_to_file(out_file.to_str().unwrap()).unwrap();
            }
            let locals = PyDict::new(*self.py);
            let anki_col = self.col;
            locals.set_item("col", anki_col).unwrap();
            locals
                .set_item(
                    "outfile",
                    PyString::new(*self.py, out_file.to_str().unwrap()),
                )
                .unwrap();
            let code = r#"
import anki
import anki.importing.apkg
importer = anki.importing.apkg.AnkiPackageImporter(col, outfile)
importer.run()
res = col
        "#;
            self.py.run(code, None, Some(locals)).unwrap();
            let col = locals.get_item("res").unwrap();
            self.col = col;
        }

        fn check_col(&mut self, condition_str: &str) -> bool {
            let code = format!(
                r#"
def assertion(col):
    return {}
        "#,
                condition_str
            );
            let assertion =
                PyModule::from_code(*self.py, &code, "assertion", "assertion.py").unwrap();
            assertion
                .call1("assertion", (self.col,))
                .unwrap()
                .extract()
                .unwrap()
        }

        fn check_media(&self) -> (Vec<String>, Vec<String>, Vec<String>) {
            let code = r#"
import os
def check_media(col):
    # col.media.check seems to assume that the cwd is the media directory. So this helper function
    # chdirs to the media dir before running check and then goes back to the original cwd.
    orig_cwd = os.getcwd()
    os.chdir(col.media.dir())
    res = col.media.check()
    os.chdir(orig_cwd)
    return res.missing, res.report, res.unused
            "#;
            let check = PyModule::from_code(*self.py, code, "check_media", "check_media.py")
                .unwrap()
                .to_owned();
            check
                .call1("check_media", (self.col,))
                .unwrap()
                .extract()
                .unwrap()
        }
    }

    #[test]
    #[serial]
    fn import_anki() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        py.import("anki").unwrap();
    }

    #[test]
    #[serial]
    fn generated_deck_can_be_imported() {
        Python::with_gil(|py| {
            let mut setup = TestSetup::new(&py);
            let mut deck = Deck::new(123456, "foodeck", "");
            deck.add_note(Note::new(model(), vec!["a", "b"]).unwrap());
            setup.import_package(Package::new(vec![deck], vec![]).unwrap(), None);
            assert!(
                setup.check_col("len(col.decks.all()) == 2 and {i['name'] for i in col.decks.all()} ==  {'Default', 'foodeck'}")
            );
        });
    }

    #[test]
    #[serial]
    fn generated_deck_has_valid_cards() {
        Python::with_gil(|py| {
            let mut setup = TestSetup::new(&py);
            let mut deck = Deck::new(123456, "foodeck", "");
            deck.add_note(Note::new(cn_model(), vec!["a", "b", "c"]).unwrap());
            deck.add_note(Note::new(cn_model(), vec!["d", "e", "f"]).unwrap());
            deck.add_note(Note::new(cn_model(), vec!["g", "h", "i"]).unwrap());
            setup.import_package(Package::new(vec![deck], vec![]).unwrap(), None);
            assert!(setup.check_col("len([col.getCard(i) for i in col.findCards('')]) == 6"));
        });
    }

    #[test]
    #[serial]
    fn multi_deck_package() {
        Python::with_gil(|py| {
            let mut setup = TestSetup::new(&py);
            let mut deck1 = Deck::new(123456, "foodeck", "");
            let mut deck2 = Deck::new(654321, "bardeck", "");
            let note = Note::new(model(), vec!["a", "b"]).unwrap();
            deck1.add_note(note.clone());
            deck2.add_note(note);
            setup.import_package(Package::new(vec![deck1, deck2], vec![]).unwrap(), None);
            assert!(setup.check_col("len(col.decks.all()) == 3"));
        });
    }

    #[test]
    fn model_req() {
        let req = model().req().unwrap();
        assert_eq!(
            req,
            vec![vec![
                Req::Integer(0),
                Req::String("all".to_string()),
                Req::IntegerArray(vec![0])
            ]]
        );
    }

    #[test]
    fn model_req_cn() {
        let req = cn_model().req().unwrap();
        assert_eq!(
            req,
            vec![
                vec![
                    Req::Integer(0),
                    Req::String("all".to_string()),
                    Req::IntegerArray(vec![0])
                ],
                vec![
                    Req::Integer(1),
                    Req::String("all".to_string()),
                    Req::IntegerArray(vec![1])
                ]
            ]
        );
    }

    #[test]
    fn model_req_with_hint() {
        let req = model_with_hint().req().unwrap();
        assert_eq!(
            req,
            vec![vec![
                Req::Integer(0),
                Req::String("any".to_string()),
                Req::IntegerArray(vec![0, 1])
            ]]
        );
    }

    #[test]
    fn notes_generate_cards_based_on_req_cn() {
        let note1 = Note::new(cn_model(), vec!["中國", "中国", "China"]).unwrap();
        let note2 = Note::new(cn_model(), vec!["你好", "", "hello"]).unwrap();

        assert_eq!(note1.cards().len(), 2);
        assert_eq!(note1.cards()[0].ord(), 0);
        assert_eq!(note1.cards()[1].ord(), 1);

        assert_eq!(note2.cards().len(), 1);
        assert_eq!(note2.cards()[0].ord(), 0)
    }

    #[test]
    fn note_generate_cards_based_on_req_with_hint() {
        let note1 = Note::new(
            model_with_hint(),
            vec!["capital of California", "", "Sacramento"],
        )
        .unwrap();
        let note2 = Note::new(
            model_with_hint(),
            vec!["capital of Iowa", "French for \"The Moines\"", "Des Moines"],
        )
        .unwrap();

        assert_eq!(note1.cards().len(), 1);
        assert_eq!(note1.cards()[0].ord(), 0);
        assert_eq!(note2.cards().len(), 1);
        assert_eq!(note2.cards()[0].ord(), 0);
    }

    #[test]
    #[serial]
    fn media_files() {
        let tmp_dir = TempDir::new().unwrap();
        std::env::set_current_dir(tmp_dir.path()).unwrap();

        let mut deck = Deck::new(123456, "foodeck", "");
        let note = Note::new(
            model(),
            vec![
                "question [sound:present.mp3] [sound:missing.mp3]",
                r#"answer <img src="present.jpg"> <img src="missing.jpg">"#,
            ],
        )
        .unwrap();
        deck.add_note(note);
        std::fs::File::create("present.mp3")
            .unwrap()
            .write(VALID_MP3)
            .unwrap();
        std::fs::File::create("present.jpg")
            .unwrap()
            .write(VALID_JPG)
            .unwrap();
        Python::with_gil(|py| {
            let mut setup = TestSetup::new(&py);
            setup.import_package(
                Package::new(vec![deck], vec!["present.mp3", "present.jpg"]).unwrap(),
                None,
            );

            std::fs::remove_file("present.mp3").unwrap();
            std::fs::remove_file("present.jpg").unwrap();

            let (missing, _, _) = setup.check_media();
            assert_eq!(missing.len(), 2);
            assert!(missing.contains(&"missing.jpg".to_string()));
            assert!(missing.contains(&"missing.mp3".to_string()));
        });
    }

    #[test]
    #[serial]
    fn media_files_absolute_paths() {
        let tmp_dir = TempDir::new().unwrap();
        std::env::set_current_dir(tmp_dir.path()).unwrap();

        let mut deck = Deck::new(123456, "foodeck", "");
        let note = Note::new(
            model(),
            vec![
                "question [sound:present.mp3] [sound:missing.mp3]",
                r#"answer <img src="present.jpg"> <img src="missing.jpg">"#,
            ],
        )
        .unwrap();
        deck.add_note(note);
        let present_mp3_path = tmp_dir.path().join("present.mp3");
        let present_jpg_path = tmp_dir.path().join("present.jpg");
        std::fs::File::create(present_mp3_path.clone())
            .unwrap()
            .write(VALID_MP3)
            .unwrap();
        std::fs::File::create(present_jpg_path.clone())
            .unwrap()
            .write(VALID_JPG)
            .unwrap();
        Python::with_gil(|py| {
            let mut setup = TestSetup::new(&py);
            setup.import_package(
                Package::new(
                    vec![deck],
                    vec![
                        present_mp3_path.to_str().unwrap(),
                        present_jpg_path.to_str().unwrap(),
                    ],
                )
                .unwrap(),
                None,
            );
            let (missing, _, _) = setup.check_media();
            assert_eq!(missing.len(), 2);
            assert!(missing.contains(&"missing.jpg".to_string()));
            assert!(missing.contains(&"missing.mp3".to_string()));
        });
    }
}
