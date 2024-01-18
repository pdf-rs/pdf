use pdf::content::ViewRect;
use pdf::error::PdfError;
use pdf::file::FileOptions;
use pdf::object::Resolve;
use pdf::primitive::{Dictionary, Primitive};
use std::env::args;

/// Extract data from a page entry that is under "other".
/// This example looks for stikethroughs in the annotations entry
/// and returns a Vec<Rect> for the bounds of the struckthrough text.
#[cfg(feature="cache")]
fn main() -> Result<(), PdfError> {
    let path = args()
        .nth(1)
        .expect("Please provide a file path to the PDF you want to explore.");

    let file = FileOptions::cached().open(&path).unwrap();
    let resolver = file.resolver();

    for (i, page) in file.pages().enumerate() {
        let page = page.unwrap();
        let strikethroughs = annotation_strikethrough(&page.other, &resolver)?;
        println!(
            "Found {} strikethrough annotations on page {}.",
            strikethroughs.len(),
            i + 1
        );
        for strikethrough in strikethroughs {
            println!();
            println!("Struck text:");
            println!("{:#?}", strikethrough.0);
            println!();
            println!("Text spans {} lines", strikethrough.1.len());
            println!();
            println!("Strikethrough bounding boxes:");
            for rect in strikethrough.1 {
                println!("{:#?}", rect);
                println!();
            }
            println!();
            println!();
        }
    }

    Ok(())
}

fn annotation_strikethrough(
    other_dict: &Dictionary,
    resolver: &impl Resolve,
) -> Result<Vec<(String, Vec<pdf::content::ViewRect>)>, PdfError> {
    let mut strikethroughs: Vec<(String, Vec<pdf::content::ViewRect>)> = Vec::new();

    if !other_dict.is_empty() {
        let annotations = other_dict.get("Annots".into());
        if let Some(annotations) = annotations {
            let annotations_resolved = annotations.clone().resolve(resolver)?;
            let annotations_array = annotations_resolved.into_array()?;
            for annotation in annotations_array.iter() {
                let mut paths: Vec<pdf::content::ViewRect> = Vec::new();
                let annotation_resolved = annotation.clone().resolve(resolver)?;
                let annotation_dict = annotation_resolved.into_dictionary()?;

                // If you have multiline strikethrough "Rect" will be the bounding
                // box around all the strikethrough lines.
                // "QuadPoints" gives 8 points for each line that is struckthrough,
                // so if a single annotation involves text on two lines, QuadPoints
                // should have 16 values in it. It starts with bottom left and
                // runs counter-clockwise.
                let subtype = annotation_dict.get("Subtype".into());
                if let Some(subtype) = subtype {
                    let subtype = subtype.clone().into_name()?;
                    if subtype.as_str() == "StrikeOut" {
                        let rects = annotation_dict.get("QuadPoints".into());
                        let text = annotation_dict.get("Contents".into());
                        if let (Some(rects), Some(text)) = (rects, text) {
                            let text = text.to_string()?;

                            // Check multiples of 8.
                            let rects_array = rects.clone().into_array()?;
                            if rects_array.len() % 8 == 0 {
                                let rects: Vec<Vec<Primitive>> =
                                    rects_array.chunks(8).map(|chunk| chunk.to_vec()).collect();

                                for rect in rects {
                                    let mut quad_points: Vec<f32> = Vec::new();
                                    for num in rect {
                                        let number = num.as_number()?;
                                        quad_points.push(number);
                                    }
                                    if quad_points.len() == 8 {
                                        paths.push(ViewRect {
                                            x: quad_points[0],
                                            y: quad_points[1],
                                            width: quad_points[2] - quad_points[0],
                                            height: quad_points[7] - quad_points[1],
                                        });
                                    }
                                }
                                strikethroughs.push((text, paths))
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(strikethroughs)
}
