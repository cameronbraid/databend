//  Copyright 2022 Datafuse Labs.
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

use std::collections::HashSet;
use std::sync::Arc;

use common_catalog::table_context::TableContext;
use common_datavalues::DataSchemaRef;
use common_exception::Result;
use common_planners::Expression;
use common_planners::ExpressionVisitor;
use common_planners::Recursion;
use common_tracing::tracing;
use opendal::Operator;

use crate::storages::fuse::io::load_bloom_filter_by_columns;
use crate::storages::fuse::io::TableMetaLocationGenerator;
use crate::storages::index::BloomFilterIndexer;

#[async_trait::async_trait]
pub trait BloomFilterPruner {
    // returns ture, if target should NOT be pruned (false positive allowed)
    async fn should_keep(&self, bloom_filter_block_path: &str) -> bool;
}

/// dummy pruner that prunes nothing
pub(crate) struct NonPruner;

#[async_trait::async_trait]
impl BloomFilterPruner for NonPruner {
    async fn should_keep(&self, _loc: &str) -> bool {
        true
    }
}

struct BloomFilterIndexPruner {
    ctx: Arc<dyn TableContext>,
    // columns that should be loaded from bloom filter block
    index_columns: Vec<String>,
    // the expression that would be evaluate
    filter_expression: Expression,
    // the data accessor
    dal: Operator,
    // the schema of data being indexed
    data_schema: DataSchemaRef,
}

impl BloomFilterIndexPruner {
    pub fn new(
        ctx: Arc<dyn TableContext>,
        index_columns: Vec<String>,
        filter_expression: Expression,
        dal: Operator,
        data_schema: DataSchemaRef,
    ) -> Self {
        Self {
            ctx,
            index_columns,
            filter_expression,
            dal,
            data_schema,
        }
    }
}

use self::util::*;
#[async_trait::async_trait]
impl BloomFilterPruner for BloomFilterIndexPruner {
    async fn should_keep(&self, loc: &str) -> bool {
        // load bloom filter index, and try pruning according to filter expression
        match filter_block_by_bloom_index(
            self.ctx.clone(),
            self.dal.clone(),
            &self.data_schema,
            &self.filter_expression,
            &self.index_columns,
            loc,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                // swallow exceptions intentionally, corrupted index should not prevent execution
                tracing::warn!("failed to apply bloom filter, returning ture. {}", e);
                true
            }
        }
    }
}

/// try to build the pruner.
/// if `filter_expr` is none, or is not applicable, e.g. have no point queries
/// a [NonPruner] will be return, which prunes nothing.
/// otherwise, a [BloomFilterIndexer] backed pruner will be return
pub fn new_bloom_filter_pruner(
    ctx: &Arc<dyn TableContext>,
    filter_expr: Option<&Expression>,
    schema: &DataSchemaRef,
    dal: Operator,
) -> Result<Arc<dyn BloomFilterPruner + Send + Sync>> {
    if let Some(expr) = filter_expr {
        // check if there were applicable filter conditions
        let point_query_cols = columns_names_of_eq_expressions(expr)?;
        if !point_query_cols.is_empty() {
            // convert to bloom filter block's column names
            let filter_block_cols = point_query_cols
                .into_iter()
                .map(|n| BloomFilterIndexer::to_bloom_column_name(&n))
                .collect();
            return Ok(Arc::new(BloomFilterIndexPruner::new(
                ctx.clone(),
                filter_block_cols,
                expr.clone(),
                dal,
                schema.clone(),
            )));
        } else {
            tracing::debug!("no point filters found, using NonPruner");
        }
    }
    Ok(Arc::new(NonPruner))
}

mod util {
    use super::*;
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn filter_block_by_bloom_index(
        ctx: Arc<dyn TableContext>,
        dal: Operator,
        schema: &DataSchemaRef,
        filter_expr: &Expression,
        bloom_index_col_names: &[String],
        block_path: &str,
    ) -> Result<bool> {
        let bloom_idx_location = TableMetaLocationGenerator::block_bloom_index_location(block_path);

        // load the relevant index columns
        let filter_block = load_bloom_filter_by_columns(
            ctx.clone(),
            dal,
            bloom_index_col_names,
            &bloom_idx_location,
        )
        .await?;

        // figure it out
        BloomFilterIndexer::from_bloom_block(schema.clone(), filter_block, ctx)?
            .maybe_true(filter_expr)
    }

    struct PointQueryVisitor {
        // names of columns which used by point query kept here
        columns: HashSet<String>,
    }

    impl ExpressionVisitor for PointQueryVisitor {
        fn pre_visit(mut self, expr: &Expression) -> Result<Recursion<Self>> {
            // TODO
            // 1. only binary op "=" is considered, which is NOT enough
            // 2. should combine this logic with BloomFilterIndexer
            match expr {
                Expression::BinaryExpression { left, op, right } if op.as_str() == "=" => {
                    match (left.as_ref(), right.as_ref()) {
                        (Expression::Column(column), Expression::Literal { .. })
                        | (Expression::Literal { .. }, Expression::Column(column)) => {
                            self.columns.insert(column.clone());
                            Ok(Recursion::Stop(self))
                        }
                        _ => Ok(Recursion::Continue(self)),
                    }
                }
                _ => Ok(Recursion::Continue(self)),
            }
        }
    }

    pub fn columns_names_of_eq_expressions(filter_expr: &Expression) -> Result<Vec<String>> {
        let visitor = PointQueryVisitor {
            columns: HashSet::new(),
        };

        filter_expr
            .accept(visitor)
            .map(|r| r.columns.into_iter().collect())
    }
}
