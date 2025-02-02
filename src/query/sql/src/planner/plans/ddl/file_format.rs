// Copyright 2023 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt::Debug;
use std::sync::Arc;

use common_expression::types::DataType;
use common_expression::DataField;
use common_expression::DataSchema;
use common_expression::DataSchemaRef;
use common_expression::DataSchemaRefExt;
use common_meta_app::principal::FileFormatOptions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateFileFormatPlan {
    pub if_not_exists: bool,
    pub name: String,
    pub file_format_options: FileFormatOptions,
}

impl CreateFileFormatPlan {
    pub fn schema(&self) -> DataSchemaRef {
        Arc::new(DataSchema::empty())
    }
}

/// Drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropFileFormatPlan {
    pub if_exists: bool,
    pub name: String,
}

impl DropFileFormatPlan {
    pub fn schema(&self) -> DataSchemaRef {
        Arc::new(DataSchema::empty())
    }
}

// Show
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowFileFormatsPlan {}

impl ShowFileFormatsPlan {
    pub fn schema(&self) -> DataSchemaRef {
        DataSchemaRefExt::create(vec![
            DataField::new("name", DataType::String),
            DataField::new("format_options", DataType::String),
        ])
    }
}
