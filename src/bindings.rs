// Derived in part from duckdb/extension-template-rs (commit abb7b2f3f954a47f90fc37a99cda3ce0618e6094)

use crate::core::{DailyRecord, DailyRequest, DuckAkiError, DELIVERED_CAPABILITIES};
use crate::eastmoney;
use chrono::NaiveDate;
use duckdb::core::{DataChunkHandle, Inserter, LogicalTypeHandle, LogicalTypeId};
use duckdb::ffi::{
    duckdb_bind_info, duckdb_client_context, duckdb_client_context_get_config_option,
    duckdb_config_option_scope, duckdb_date, duckdb_destroy_client_context, duckdb_destroy_value,
    duckdb_get_bool, duckdb_get_date, duckdb_is_null_value, duckdb_table_function_get_client_context,
    duckdb_timestamp, duckdb_value,
};
use duckdb::vtab::{BindInfo, InitInfo, TableFunctionInfo, VTab, Value};
use duckdb::Connection;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

// -------------------------------------------------------------
// Client Context & Security
// -------------------------------------------------------------

#[repr(C)]
struct RawBindInfo {
    ptr: duckdb_bind_info,
}

#[repr(C)]
struct RawValue {
    ptr: duckdb_value,
}

pub struct ClientContextWrapper {
    raw: duckdb_client_context,
}

unsafe impl Send for ClientContextWrapper {}
unsafe impl Sync for ClientContextWrapper {}

impl Drop for ClientContextWrapper {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                duckdb_destroy_client_context(&mut self.raw);
            }
        }
    }
}

impl ClientContextWrapper {
    pub fn is_external_access_enabled(&self) -> bool {
        if self.raw.is_null() {
            return false;
        }
        unsafe {
            let opt_name = CString::new("enable_external_access").unwrap();
            let mut scope: duckdb_config_option_scope = 0;
            let val = duckdb_client_context_get_config_option(
                self.raw,
                opt_name.as_ptr(),
                &mut scope,
            );
            if val.is_null() || duckdb_is_null_value(val) {
                return false;
            }
            let res = duckdb_get_bool(val);
            duckdb_destroy_value(&mut { val });
            res
        }
    }
}

fn extract_client_context(bind: &BindInfo) -> Result<Arc<ClientContextWrapper>, DuckAkiError> {
    unsafe {
        let raw = &*(bind as *const BindInfo as *const RawBindInfo);
        let mut ctx: duckdb_client_context = std::ptr::null_mut();
        duckdb_table_function_get_client_context(raw.ptr, &mut ctx);
        if ctx.is_null() {
            return Err(DuckAkiError::ExternalAccessDisabled(
                "failed to obtain client context".to_string(),
            ));
        }
        Ok(Arc::new(ClientContextWrapper { raw: ctx }))
    }
}

fn duckdb_date_to_naive(v: &Value) -> Result<NaiveDate, DuckAkiError> {
    unsafe {
        let raw = &*(v as *const Value as *const RawValue);
        if raw.ptr.is_null() || duckdb_is_null_value(raw.ptr) {
            return Err(DuckAkiError::InvalidArgument("date cannot be NULL".to_string()));
        }
        let d = duckdb_get_date(raw.ptr);
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        epoch
            .checked_add_signed(chrono::Duration::days(d.days as i64))
            .ok_or_else(|| DuckAkiError::InvalidArgument(format!("date days out of range: {}", d.days)))
    }
}

fn naive_to_duckdb_date(d: NaiveDate) -> duckdb_date {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let days = (d - epoch).num_days() as i32;
    duckdb_date { days }
}

// -------------------------------------------------------------
// aki_stock_daily VTab
// -------------------------------------------------------------

pub struct StockDailyBindData {
    pub request: DailyRequest,
    pub context: Arc<ClientContextWrapper>,
}

pub struct StockDailyInitData {
    pub records: Vec<DailyRecord>,
    pub cursor: AtomicUsize,
    pub done: AtomicBool,
}

pub struct StockDailyVTab;

impl VTab for StockDailyVTab {
    type InitData = StockDailyInitData;
    type BindData = StockDailyBindData;

    fn bind(bind: &BindInfo) -> Result<Self::BindData, Box<dyn std::error::Error>> {
        // 1. Check external access via client context
        let context = extract_client_context(bind).map_err(Box::new)?;
        if !context.is_external_access_enabled() {
            return Err(Box::new(DuckAkiError::ExternalAccessDisabled(
                "external access is disabled in DuckDB configuration".to_string(),
            )));
        }

        // 2. Positional parameters: symbol (0), start_date (1), end_date (2)
        let sym_val = bind.get_parameter(0);
        if sym_val.is_null() {
            return Err(Box::new(DuckAkiError::InvalidArgument(
                "symbol parameter cannot be NULL".to_string(),
            )));
        }
        let symbol = sym_val.to_string();

        let start_val = bind.get_parameter(1);
        if start_val.is_null() {
            return Err(Box::new(DuckAkiError::InvalidArgument(
                "start_date parameter cannot be NULL".to_string(),
            )));
        }
        let start_date = duckdb_date_to_naive(&start_val).map_err(Box::new)?;

        let end_val = bind.get_parameter(2);
        if end_val.is_null() {
            return Err(Box::new(DuckAkiError::InvalidArgument(
                "end_date parameter cannot be NULL".to_string(),
            )));
        }
        let end_date = duckdb_date_to_naive(&end_val).map_err(Box::new)?;

        // 3. Named parameter: source
        let source_val = match bind.get_named_parameter("source") {
            Some(v) => {
                if v.is_null() {
                    return Err(Box::new(DuckAkiError::InvalidArgument(
                        "source parameter is required and cannot be NULL".to_string(),
                    )));
                }
                v.to_string()
            }
            None => {
                return Err(Box::new(DuckAkiError::InvalidArgument(
                    "source parameter is required and must be specified".to_string(),
                )));
            }
        };

        // 4. Validate with DailyRequest
        let request = DailyRequest::new(symbol, start_date, end_date, source_val)?;

        // 5. Declare 12 result columns
        bind.add_result_column("symbol", LogicalTypeHandle::from(LogicalTypeId::Varchar));
        bind.add_result_column("trade_date", LogicalTypeHandle::from(LogicalTypeId::Date));
        bind.add_result_column("open", LogicalTypeHandle::from(LogicalTypeId::Double));
        bind.add_result_column("high", LogicalTypeHandle::from(LogicalTypeId::Double));
        bind.add_result_column("low", LogicalTypeHandle::from(LogicalTypeId::Double));
        bind.add_result_column("close", LogicalTypeHandle::from(LogicalTypeId::Double));
        bind.add_result_column(
            "volume_shares",
            LogicalTypeHandle::from(LogicalTypeId::Bigint),
        );
        bind.add_result_column("amount", LogicalTypeHandle::from(LogicalTypeId::Double));
        bind.add_result_column("currency", LogicalTypeHandle::from(LogicalTypeId::Varchar));
        bind.add_result_column("source", LogicalTypeHandle::from(LogicalTypeId::Varchar));
        bind.add_result_column(
            "adjustment",
            LogicalTypeHandle::from(LogicalTypeId::Varchar),
        );
        bind.add_result_column(
            "fetched_at",
            LogicalTypeHandle::from(LogicalTypeId::TimestampTZ),
        );

        let days = (request.end_date - request.start_date).num_days() + 1;
        bind.set_cardinality(days as u64, false);

        Ok(StockDailyBindData { request, context })
    }

    fn init(init: &InitInfo) -> Result<Self::InitData, Box<dyn std::error::Error>> {
        init.set_max_threads(1);

        let bind_data = unsafe { &*init.get_bind_data::<StockDailyBindData>() };

        // Re-check external access prior to execution (prevents prepared statement bypass)
        if !bind_data.context.is_external_access_enabled() {
            return Err(Box::new(DuckAkiError::ExternalAccessDisabled(
                "external access was disabled prior to execution".to_string(),
            )));
        }

        // Fetch, parse, validate, then stamp and expose one complete batch.
        let batch = eastmoney::fetch_and_validate(&bind_data.request)?;

        Ok(StockDailyInitData {
            records: batch.records,
            cursor: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        })
    }

    fn func(
        func: &TableFunctionInfo<Self>,
        output: &mut DataChunkHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let init_data = func.get_init_data();

        if init_data.done.load(Ordering::SeqCst) {
            output.set_len(0);
            return Ok(());
        }

        let total = init_data.records.len();
        let current_cursor = init_data.cursor.load(Ordering::SeqCst);
        if current_cursor >= total {
            output.set_len(0);
            init_data.done.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let remaining = total - current_cursor;
        let to_emit = remaining.min(2048);

        let vec_symbol = output.flat_vector(0);
        let mut vec_trade_date = output.flat_vector(1);
        let mut vec_open = output.flat_vector(2);
        let mut vec_high = output.flat_vector(3);
        let mut vec_low = output.flat_vector(4);
        let mut vec_close = output.flat_vector(5);
        let mut vec_volume_shares = output.flat_vector(6);
        let mut vec_amount = output.flat_vector(7);
        let vec_currency = output.flat_vector(8);
        let vec_source = output.flat_vector(9);
        let vec_adjustment = output.flat_vector(10);
        let mut vec_fetched_at = output.flat_vector(11);

        for row_idx in 0..to_emit {
            let record = &init_data.records[current_cursor + row_idx];

            // 0: symbol
            vec_symbol.insert(row_idx, record.symbol.as_str());

            // 1: trade_date
            unsafe {
                vec_trade_date.as_mut_slice_with_len::<duckdb_date>(to_emit)[row_idx] =
                    naive_to_duckdb_date(record.trade_date);
            }

            // 2-5: OHLC
            unsafe {
                vec_open.as_mut_slice_with_len::<f64>(to_emit)[row_idx] = record.open;
                vec_high.as_mut_slice_with_len::<f64>(to_emit)[row_idx] = record.high;
                vec_low.as_mut_slice_with_len::<f64>(to_emit)[row_idx] = record.low;
                vec_close.as_mut_slice_with_len::<f64>(to_emit)[row_idx] = record.close;
            }

            // 6: volume_shares (nullable)
            if let Some(vol) = record.volume_shares {
                unsafe {
                    vec_volume_shares.as_mut_slice_with_len::<i64>(to_emit)[row_idx] = vol;
                }
            } else {
                vec_volume_shares.set_null(row_idx);
            }

            // 7: amount (nullable)
            if let Some(amt) = record.amount {
                unsafe {
                    vec_amount.as_mut_slice_with_len::<f64>(to_emit)[row_idx] = amt;
                }
            } else {
                vec_amount.set_null(row_idx);
            }

            // 8: currency
            vec_currency.insert(row_idx, record.currency.as_str());

            // 9: source
            vec_source.insert(row_idx, record.source.as_str());

            // 10: adjustment
            vec_adjustment.insert(row_idx, record.adjustment.as_str());

            // 11: fetched_at
            unsafe {
                vec_fetched_at.as_mut_slice_with_len::<duckdb_timestamp>(to_emit)[row_idx] =
                    duckdb_timestamp {
                        micros: record.fetched_at_micros,
                    };
            }
        }

        let new_cursor = current_cursor + to_emit;
        init_data.cursor.store(new_cursor, Ordering::SeqCst);
        output.set_len(to_emit);
        if new_cursor >= total {
            init_data.done.store(true, Ordering::SeqCst);
        }

        Ok(())
    }

    fn parameters() -> Option<Vec<LogicalTypeHandle>> {
        Some(vec![
            LogicalTypeHandle::from(LogicalTypeId::Varchar),
            LogicalTypeHandle::from(LogicalTypeId::Date),
            LogicalTypeHandle::from(LogicalTypeId::Date),
        ])
    }

    fn named_parameters() -> Option<Vec<(String, LogicalTypeHandle)>> {
        Some(vec![(
            "source".to_string(),
            LogicalTypeHandle::from(LogicalTypeId::Varchar),
        )])
    }
}

// -------------------------------------------------------------
// aki_datasets VTab
// -------------------------------------------------------------

pub struct DatasetsInitData {
    pub cursor: AtomicUsize,
    pub done: AtomicBool,
}

pub struct DatasetsVTab;

impl VTab for DatasetsVTab {
    type InitData = DatasetsInitData;
    type BindData = ();

    fn bind(bind: &BindInfo) -> Result<Self::BindData, Box<dyn std::error::Error>> {
        bind.add_result_column("dataset", LogicalTypeHandle::from(LogicalTypeId::Varchar));
        bind.add_result_column(
            "function_name",
            LogicalTypeHandle::from(LogicalTypeId::Varchar),
        );
        bind.add_result_column("source", LogicalTypeHandle::from(LogicalTypeId::Varchar));
        bind.add_result_column(
            "description",
            LogicalTypeHandle::from(LogicalTypeId::Varchar),
        );
        bind.set_cardinality(DELIVERED_CAPABILITIES.len() as u64, true);
        Ok(())
    }

    fn init(init: &InitInfo) -> Result<Self::InitData, Box<dyn std::error::Error>> {
        init.set_max_threads(1);
        Ok(DatasetsInitData {
            cursor: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        })
    }

    fn func(
        func: &TableFunctionInfo<Self>,
        output: &mut DataChunkHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let init_data = func.get_init_data();

        let total = DELIVERED_CAPABILITIES.len();
        if init_data.done.load(Ordering::SeqCst) {
            output.set_len(0);
            return Ok(());
        }

        let current_cursor = init_data.cursor.load(Ordering::SeqCst);
        if current_cursor >= total {
            output.set_len(0);
            init_data.done.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let remaining = total - current_cursor;
        let to_emit = remaining.min(2048);

        let vec_dataset = output.flat_vector(0);
        let vec_function_name = output.flat_vector(1);
        let vec_source = output.flat_vector(2);
        let vec_description = output.flat_vector(3);

        for row_idx in 0..to_emit {
            let cap = &DELIVERED_CAPABILITIES[current_cursor + row_idx];
            vec_dataset.insert(row_idx, cap.dataset);
            vec_function_name.insert(row_idx, cap.function_name);
            vec_source.insert(row_idx, cap.source);
            vec_description.insert(row_idx, cap.description);
        }

        let new_cursor = current_cursor + to_emit;
        init_data.cursor.store(new_cursor, Ordering::SeqCst);
        output.set_len(to_emit);
        if new_cursor >= total {
            init_data.done.store(true, Ordering::SeqCst);
        }

        Ok(())
    }
}

pub fn register(con: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    con.register_table_function::<StockDailyVTab>("aki_stock_daily")?;
    con.register_table_function::<DatasetsVTab>("aki_datasets")?;
    Ok(())
}
