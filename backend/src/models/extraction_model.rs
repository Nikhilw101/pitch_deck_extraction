use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub slide_number: u32,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Element {
    TextBlock {
        text: String,
        bbox: Option<BoundingBox>,
    },
    Title {
        text: String,
        bbox: Option<BoundingBox>,
    },
    Subtitle {
        text: String,
        bbox: Option<BoundingBox>,
    },
    SectionHeader {
        text: String,
        bbox: Option<BoundingBox>,
    },
    Statistic {
        value: String,
        label: String,
        bbox: Option<BoundingBox>,
    },
    BulletList {
        items: Vec<String>,
        level: Option<u8>,
        bbox: Option<BoundingBox>,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        bbox: Option<BoundingBox>,
    },
    Image {
        path: PathBuf,
        bbox: Option<BoundingBox>,
        ocr_text: Option<String>,
    },
    Chart {
        chart_type: ChartType,
        title: Option<String>,
        data: ChartData,
        bbox: Option<BoundingBox>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    pub x_axis: Option<Vec<String>>,
    pub series: Vec<Series>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BoundingBox {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }
}
