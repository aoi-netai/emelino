use super::*;
use crate::contract::runtime::{RuntimeHelper as Helper, RuntimeRequirements as Requirements};
pub(in crate::backend::emit) trait HelperEmission: Sized {
    fn emit(self, writer: &mut Writer) -> Result<(), Vec<Error>>;
}
impl HelperEmission for Helper {
    fn emit(self, writer: &mut Writer) -> Result<(), Vec<Error>> {
        match self {
            Self::StringComparison => {
                features::primitive_operations::emit_string_comparison_helper(writer)
            }
            Self::I32FromBits => numeric_runtime::emit_i32_from_bits_helper(writer),
            Self::I32Shift => return Err(unsupported("runtime", "runtime helper")),
            Self::NegativeShift => numeric_runtime::emit_negative_shift_helper(writer),
            Self::I32Division => features::primitive_operations::emit_i32_division_helpers(writer),
            Self::I64FromBits => numeric_runtime::emit_i64_wrap_helper(writer),
            Self::I64Shift => return Err(unsupported("runtime", "runtime helper")),
            Self::I64Division => features::primitive_operations::emit_i64_division_helpers(writer),
            Self::FloatToInt => return Err(unsupported("runtime", "runtime helper")),
            Self::DynamicDefect => features::terms::defect::emit_dynamic_defect_kind_helper(writer),
            Self::StringMatchesAt => return Err(unsupported("runtime", "runtime helper")),
            Self::StringContains => return Err(unsupported("runtime", "runtime helper")),
            Self::StringStartsWith => return Err(unsupported("runtime", "runtime helper")),
            Self::StringEndsWith => return Err(unsupported("runtime", "runtime helper")),
            Self::Utf8Decode => return Err(unsupported("runtime", "runtime helper")),
            Self::Grapheme => return Err(unsupported("runtime", "runtime helper")),
            Self::StringTrim => return Err(unsupported("runtime", "runtime helper")),
            Self::StringJoin => return Err(unsupported("runtime", "runtime helper")),
            Self::StringAppend => return Err(unsupported("runtime", "runtime helper")),
            Self::StringShow => features::runtime::emit_string_show_helpers(writer),
            Self::IntToString => return Err(unsupported("runtime", "runtime helper")),
            Self::IntParse => return Err(unsupported("runtime", "runtime helper")),
            Self::FloatParse => return Err(unsupported("runtime", "runtime helper")),
            Self::FloatToString => return Err(unsupported("runtime", "runtime helper")),
            Self::StringOutput => super::string_buffers::emit_string_output_helper(writer),
        }

        Ok(())
    }
}
pub(in crate::backend::emit) trait RuntimeEmission: Sized {
    fn emit_helpers(&self, writer: &mut Writer) -> Result<(), Vec<Error>>;
}
impl RuntimeEmission for Requirements {
    fn emit_helpers(&self, writer: &mut Writer) -> Result<(), Vec<Error>> {
        for &helper in &self.helpers {
            helper.emit(writer)?;
        }

        Ok(())
    }
}
