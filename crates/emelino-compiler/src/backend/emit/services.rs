use super::*;
use crate::contract::services::Operation;
use std::collections::BTreeSet;
fn snake(name: &str) -> String {
    let mut result = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

fn qualified(module: &str, item: &str) -> String {
    if item.starts_with(module) {
        item.into()
    } else {
        format!("{module}{item}")
    }
}

fn enum_name(core: &crate::contract::ir::Definitions, id: EnumId) -> Option<String> {
    let rest = core.enums[id].name.strip_prefix("Embedded.")?;
    let (module, item) = rest.split_once('.')?;
    Some(format!("Emela{}", qualified(module, item)))
}

fn board_type(core: &crate::contract::ir::Definitions, ty: TypeId) -> Result<String, Vec<Error>> {
    match &core.types[ty] {
        Type::Int | Type::Bool => Ok("int32_t".into()),
        Type::Int64 => Ok("int64_t".into()),
        Type::Unit => Ok("void".into()),
        Type::Enum(id, args) if args.is_empty() => enum_name(core, *id)
            .ok_or_else(|| unsupported("Embedded.Board", "non-peripheral board type")),
        _ => Err(unsupported(
            "Embedded.Board",
            "unsupported board value type",
        )),
    }
}

fn int_list(core: &crate::contract::ir::Definitions, ty: TypeId) -> bool {
    matches!(core.types[ty], Type::List(element) if element == TypeId::INT)
}

fn field_names(variant: &crate::contract::ir::VariantDef) -> Vec<String> {
    match &variant.fields {
        crate::contract::ir::VariantFields::Named(names) => names.clone(),
        _ => (0..variant.tys.len())
            .map(|index| format!("f{index}"))
            .collect(),
    }
}

fn emit_enum(
    core: &crate::contract::ir::Definitions,
    id: EnumId,
    written: &mut HashSet<EnumId>,
    writer: &mut Writer,
) -> Result<(), Vec<Error>> {
    if !written.insert(id) {
        return Ok(());
    }
    let def = &core.enums[id];
    let name = enum_name(core, id).ok_or_else(|| unsupported(&def.name, "invalid board type"))?;
    let guard = format!("EMELA_DEFINED_{}", snake(&name).to_uppercase());
    writer.line(&format!("#ifndef {guard}"));
    writer.line(&format!("#define {guard}"));
    if def.variants.iter().all(|variant| variant.tys.is_empty()) {
        writer.line(&format!("typedef int32_t {name};"));
        writer.line("enum {");
        for (index, variant) in def.variants.iter().enumerate() {
            writer.line(&format!(
                "  EMELA_{}_{} = {index},",
                snake(name.strip_prefix("Emela").unwrap()).to_uppercase(),
                snake(&variant.name).to_uppercase()
            ));
        }
        writer.line("};");
    } else if def.variants.len() == 1 {
        let variant = &def.variants[0];
        for &field in &variant.tys {
            if let Type::Enum(child, _) = core.types[field] {
                emit_enum(core, child, written, writer)?;
            }
        }
        writer.line("typedef struct {");
        for (&ty, field) in variant.tys.iter().zip(field_names(variant)) {
            writer.line(&format!("  {} {field};", board_type(core, ty)?));
        }
        writer.line(&format!("}} {name};"));
    } else {
        for variant in &def.variants {
            for &field in &variant.tys {
                if let Type::Enum(child, _) = core.types[field] {
                    emit_enum(core, child, written, writer)?;
                }
            }
        }
        writer.line("enum {");
        for (index, variant) in def.variants.iter().enumerate() {
            writer.line(&format!(
                "  EMELA_{}_{} = {index},",
                snake(name.strip_prefix("Emela").unwrap()).to_uppercase(),
                snake(&variant.name).to_uppercase()
            ));
        }
        writer.line("};");
        writer.line("typedef struct {");
        writer.line("  int32_t tag;");
        writer.line("  union {");
        for variant in &def.variants {
            if variant.tys.is_empty() {
                continue;
            }
            writer.line("    struct {");
            for (&ty, field) in variant.tys.iter().zip(field_names(variant)) {
                writer.line(&format!("      {} {field};", board_type(core, ty)?));
            }
            writer.line(&format!("    }} {};", snake(&variant.name)));
        }
        writer.line("  } payload;");
        writer.line(&format!("}} {name};"));
    }
    writer.line("#endif");
    writer.blank();
    Ok(())
}

fn prototype(
    core: &crate::contract::ir::Definitions,
    peripheral: &str,
    op: &crate::contract::ir::OpDef,
) -> Result<String, Vec<Error>> {
    let mut params = Vec::new();
    for (index, &ty) in op.params.iter().enumerate() {
        // The generated library owns closures; the BSP only configures the IRQ.
        if peripheral == "gpio" && op.name == "enable_interrupt" && index == 3 {
            continue;
        }
        if int_list(core, ty) {
            params.extend([
                format!("const uint16_t *p{index}"),
                format!("uint16_t p{index}_length"),
            ]);
        } else {
            params.push(format!("{} p{index}", board_type(core, ty)?));
        }
    }
    let returns_list = int_list(core, op.ret);
    let ret = if op.fails {
        if returns_list {
            params.extend([
                "uint16_t *out".into(),
                "uint16_t capacity".into(),
                "uint16_t *length".into(),
            ]);
        } else if op.ret != TypeId::UNIT {
            params.push(format!("{} *out", board_type(core, op.ret)?));
        }
        "int32_t".into()
    } else {
        board_type(core, op.ret)?
    };
    Ok(format!(
        "{ret} emela_{peripheral}_{}({});",
        op.name,
        if params.is_empty() {
            "void".into()
        } else {
            params.join(", ")
        }
    ))
}

pub(super) fn headers(
    flow: &crate::contract::ir::Program,
    operations: &HashMap<FnId, Operation>,
    capacity: u16,
) -> Result<Vec<GeneratedFile>, Vec<Error>> {
    let mut used = BTreeSet::new();
    for operation in operations.values() {
        used.insert(operation.peripheral.clone());
    }
    let mut files = Vec::new();
    if used
        .iter()
        .any(|name| matches!(name.as_str(), "gpio" | "uart" | "i2c" | "spi"))
    {
        files.push(GeneratedFile { path: "emela_peripheral.h".into(), contents: "#ifndef EMELA_PERIPHERAL_H\n#define EMELA_PERIPHERAL_H\n#include <stdint.h>\n/* Reserved bridge errors; board implementations must use different nonzero codes. */\n#define EMELA_PERIPHERAL_INVALID_ARGUMENT (-INT32_C(1))\n#define EMELA_PERIPHERAL_CAPACITY_EXCEEDED (-INT32_C(2))\n#endif\n".into() });
    }
    for peripheral in used {
        let mut writer = Writer::new();
        let guard = format!("EMELA_{}_H", peripheral.to_uppercase());
        writer.line(&format!("#ifndef {guard}"));
        writer.line(&format!("#define {guard}"));
        writer.line("#include <stdint.h>");
        match peripheral.as_str() {
            "clock" => {
                writer.line("#include \"emela_embedded.h\"");
                writer
                    .line("/* Milliseconds since the Unix epoch; the board supplies the epoch. */");
                writer.line("int64_t emela_board_clock_now(void);");
                writer.line("#ifdef EMELA_HAS_TASK_RUNTIME");
                writer.line("/* Complete with emela_task_complete_unit from an ISR; dispatch runs in main. */");
                writer.line("void emela_board_clock_sleep(EmelaTask task, int32_t milliseconds);");
                writer.line("#endif");
            }
            "console" => {
                writer.line(
                    "/* Borrowed UTF-8 bytes; append one newline. Do not retain the pointer. */",
                );
                writer.line(
                    "void emela_board_console_print_line(const char *data, uint16_t length);",
                );
                writer.line(
                    "void emela_board_console_print_error_line(const char *data, uint16_t length);",
                );
            }
            _ => {
                writer.line("#include \"emela_peripheral.h\"");
                if peripheral == "gpio" {
                    writer.line("/* Configure/register/unregister in main context; read/write/toggle are ISR-safe. */");
                    writer.line("/* The platform event lock must exclude all GPIO dispatch while registrations change. */");
                } else {
                    writer.line("/* Synchronous, main-context operations. Arrays are borrowed only during the call. */");
                    writer.line("/* Words preserve 0..65535. The board must reject values outside its configured 7/8/9/16-bit width, never truncate. */");
                }
                writer.line("/* Return 0 on success. On failure output values are ignored. Never exceed capacity. */");
                let module = match peripheral.as_str() {
                    "gpio" => "Gpio",
                    "uart" => "Uart",
                    "i2c" => "I2c",
                    _ => "Spi",
                };
                let prefix = format!("Embedded.{module}.");
                writer.line(&format!("#ifndef EMELA_PERIPHERAL_BUFFER_CAPACITY\n#define EMELA_PERIPHERAL_BUFFER_CAPACITY {capacity}\n#endif"));
                let mut written = HashSet::new();
                for (id, _) in
                    flow.core.enums.iter().filter(|(_, def)| {
                        def.name.starts_with(&prefix) && !def.error && !def.external
                    })
                {
                    emit_enum(&flow.core, id, &mut written, &mut writer)?;
                }
                if matches!(peripheral.as_str(), "gpio" | "uart" | "i2c") {
                    if let Some((id, _)) = flow
                        .core
                        .enums
                        .iter()
                        .find(|(_, def)| def.name == "Embedded.Interrupt.InterruptPriority")
                    {
                        emit_enum(&flow.core, id, &mut written, &mut writer)?;
                    }
                }
                if peripheral == "gpio" {
                    writer.line(
                        "/* GPIO status: 0 = success; GpioErrorKind ordinal + 1 = failure. */",
                    );
                    if flow
                        .core
                        .enums
                        .iter()
                        .any(|(_, def)| def.name == "Embedded.Gpio.GpioConfig")
                    {
                        writer.line("#define EMELA_GPIO_ABI_VERSION 1");
                        writer.line(&format!(
                            "#define EMELA_GPIO_INTERRUPT_CAPACITY {GPIO_CALLBACK_CAPACITY}"
                        ));
                    }
                    writer.line("/* BSP calls this after acknowledging the IRQ; an unregistered pin is ignored. */");
                    writer.line("void emela_gpio_dispatch_interrupt(EmelaGpioPin pin, EmelaGpioPinState level);");
                }
                let effect_name = format!("{prefix}{module}");
                if let Some((_, effect)) = flow
                    .core
                    .effects
                    .iter()
                    .find(|(_, effect)| effect.name == effect_name)
                {
                    for op in &effect.ops {
                        writer.line(&prototype(&flow.core, &peripheral, op)?);
                    }
                }
            }
        }
        writer.line("#endif");
        files.push(GeneratedFile {
            path: format!("emela_{peripheral}.h"),
            contents: writer.finish(),
        });
    }
    Ok(files)
}

fn to_board(layouts: &TypeLayouts<'_>, ty: TypeId, expression: &str) -> Result<String, Vec<Error>> {
    if let Type::Enum(id, _) = layouts.shapes.types[ty] {
        let def = &layouts.shapes.core.enums[id];
        if !layouts.enum_is_i32(id) {
            if def.variants.len() > 1 {
                let name = enum_name(layouts.shapes.core, id).unwrap();
                let mut choices = Vec::new();
                for (variant_index, variant) in def.variants.iter().enumerate() {
                    let fields = variant
                        .tys
                        .iter()
                        .zip(field_names(variant))
                        .enumerate()
                        .map(|(index, (&field, name))| {
                            Ok(format!(
                                ".{name} = {}",
                                to_board(
                                    layouts,
                                    field,
                                    &format!("({expression}).payload.v{variant_index}.f{index}")
                                )?
                            ))
                        })
                        .collect::<Result<Vec<_>, Vec<Error>>>()?;
                    let payload = if fields.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ", .payload.{} = {{ {} }}",
                            snake(&variant.name),
                            fields.join(", ")
                        )
                    };
                    choices.push(format!("({name}){{ .tag = {variant_index}{payload} }}"));
                }
                let mut value = choices.pop().unwrap();
                for (index, choice) in choices.into_iter().enumerate().rev() {
                    value = format!("(({expression}).tag == {index} ? {choice} : {value})");
                }
                return Ok(value);
            }
            let variant = &def.variants[0];
            let fields = variant
                .tys
                .iter()
                .zip(field_names(variant))
                .enumerate()
                .map(|(index, (&field, name))| {
                    Ok(format!(
                        ".{name} = {}",
                        to_board(
                            layouts,
                            field,
                            &format!("({expression}).payload.v0.f{index}")
                        )?
                    ))
                })
                .collect::<Result<Vec<_>, Vec<Error>>>()?;
            return Ok(format!(
                "({}){{ {} }}",
                enum_name(layouts.shapes.core, id).unwrap(),
                fields.join(", ")
            ));
        }
    }
    Ok(expression.into())
}

// A bounded subscription table belongs to the generated application, not the BSP.
// Pin IDs are opaque: allocation searches by ID rather than indexing by physical pin.
const GPIO_CALLBACK_CAPACITY: usize = 32;

pub(super) fn emit_gpio_runtime(
    operations: &HashMap<FnId, Operation>,
    layouts: &TypeLayouts<'_>,
    writer: &mut Writer,
) -> bool {
    if !operations.values().any(|op| op.peripheral == "gpio") {
        return false;
    }
    let registration = operations.values().find(|op| {
        op.peripheral == "gpio"
            && op.name == "enable_interrupt"
            && op.params.len() == 4
            && matches!(layouts.shapes.types[op.params[3]], Type::Fn(_))
    });
    if let Some(op) = registration {
        let callback = layouts.c_type(op.params[3]);
        writer.line(&format!(
            "typedef struct {{ int32_t pin; {callback} callback; }} emela_gpio_subscription;"
        ));
        writer.line(&format!(
            "static emela_gpio_subscription emela_gpio_subscriptions[{GPIO_CALLBACK_CAPACITY}];"
        ));
        writer.begin_block("static int emela_gpio_find_subscription(int32_t pin)");
        writer.line(&format!(
            "for (int i = 0; i < {GPIO_CALLBACK_CAPACITY}; ++i) {{"
        ));
        writer.line("  if (emela_gpio_subscriptions[i].callback.call != NULL && emela_gpio_subscriptions[i].pin == pin) return i;");
        writer.line("}");
        writer.ret(Some("-1"));
        writer.end_block();
    }
    writer.begin_block(
        "void emela_gpio_dispatch_interrupt(EmelaGpioPin pin, EmelaGpioPinState level)",
    );
    if let Some(op) = registration {
        let Type::Fn(callback) = &layouts.shapes.types[op.params[3]] else {
            unreachable!()
        };
        let event = layouts.c_type(callback.params[0]);
        writer.line("int slot = emela_gpio_find_subscription(pin.id);");
        writer.line("if (slot < 0) return;");
        writer.line(&format!("{event} event = {{0}};"));
        writer.line("event.payload.v0.f0.payload.v0.f0 = pin.id;");
        writer.line("event.payload.v0.f1 = level;");
        writer.line("/* Borrow the registration: no allocation, retain or release in the ISR. */");
        writer.line("emela_gpio_subscriptions[slot].callback.call(emela_gpio_subscriptions[slot].callback.env, event);");
    } else {
        writer.line("(void)pin;");
        writer.line("(void)level;");
    }
    writer.end_block();
    writer.blank();
    registration.is_some()
}

fn emit_gpio_registration(
    op: &Operation,
    layouts: &TypeLayouts<'_>,
    function: &Function,
    variables: &[String],
    writer: &mut Writer,
) {
    let pin = &variables[0];
    writer.decl(&layouts.c_type(function.ret), "result", Some("{0}"));
    writer.line("void *previous_env = NULL;");
    writer.line("uint32_t irq_state = emela_board_event_lock();");
    writer.line(&format!(
        "int slot = emela_gpio_find_subscription({pin}.payload.v0.f0);"
    ));
    if op.name == "enable_interrupt" {
        let edge = &variables[1];
        let priority = &variables[2];
        let callback = &variables[3];
        writer.begin_block("if (slot < 0)");
        writer.line(&format!(
            "for (int i = 0; i < {GPIO_CALLBACK_CAPACITY}; ++i) {{"
        ));
        writer
            .line("  if (emela_gpio_subscriptions[i].callback.call == NULL) { slot = i; break; }");
        writer.line("}");
        writer.end_block();
        writer.begin_block("if (slot < 0)");
        writer.line("result.f0 = EMELA_GPIO_ERROR_KIND_BUSY + 1;");
        writer.line("emela_board_event_unlock(irq_state);");
        writer.ret(Some("result"));
        writer.end_block();
        writer.line(&format!("result.f0 = emela_gpio_enable_interrupt((EmelaGpioPin){{ .id = {pin}.payload.v0.f0 }}, {edge}, {priority});"));
        writer.begin_block("if (result.f0 == 0)");
        writer.line(&format!("emela_heap_retain({callback}.env);"));
        writer.line("previous_env = emela_gpio_subscriptions[slot].callback.env;");
        writer.line(&format!(
            "emela_gpio_subscriptions[slot].pin = {pin}.payload.v0.f0;"
        ));
        writer.line(&format!(
            "emela_gpio_subscriptions[slot].callback = {callback};"
        ));
        writer.end_block();
    } else {
        writer.line(&format!("result.f0 = emela_gpio_disable_interrupt((EmelaGpioPin){{ .id = {pin}.payload.v0.f0 }});"));
        writer.begin_block("if (result.f0 == 0 && slot >= 0)");
        writer.line("previous_env = emela_gpio_subscriptions[slot].callback.env;");
        writer.line("emela_gpio_subscriptions[slot] = (emela_gpio_subscription){0};");
        writer.end_block();
    }
    writer.line("emela_board_event_unlock(irq_state);");
    writer.line("emela_heap_release(previous_env);");
    writer.ret(Some("result"));
}

pub(super) struct AdapterOptions {
    pub capacity: u16,
    pub gpio_callbacks: bool,
}

pub(super) fn emit_adapter(
    flow: &crate::contract::ir::Program,
    op: &Operation,
    layouts: &TypeLayouts<'_>,
    function: &Function,
    name: &str,
    options: AdapterOptions,
    writer: &mut Writer,
) -> Result<bool, Vec<Error>> {
    let AdapterOptions {
        capacity,
        gpio_callbacks,
    } = options;
    writer.begin_block(&signature(layouts, function, name, true, false, false));
    let variables: Vec<_> = function.params.iter().map(|&p| variable_name(p)).collect();
    if op.peripheral == "gpio"
        && gpio_callbacks
        && matches!(op.name.as_str(), "enable_interrupt" | "disable_interrupt")
    {
        emit_gpio_registration(op, layouts, function, &variables, writer);
        writer.end_block();
        writer.blank();
        return Ok(true);
    }
    if op.peripheral == "clock" {
        writer.ret(Some("emela_board_clock_now()"));
    } else if op.peripheral == "console" {
        writer.line(&format!(
            "emela_board_console_{}({}.data, {}.length);",
            op.name, variables[0], variables[0]
        ));
    } else {
        let mut args = Vec::new();
        if op.fails {
            writer.decl(&layouts.c_type(function.ret), "result", Some("{0}"));
        }
        for (index, (&ty, variable)) in op.params.iter().zip(&variables).enumerate() {
            if int_list(&flow.core, ty) {
                let node = layouts.list_node_name(ty);
                writer.line(&format!("uint16_t input_{index}[{capacity}];"));
                writer.line(&format!("uint16_t length_{index} = 0;"));
                writer.begin_block(&format!("for (const {node} *cursor = (const {node} *){variable}; cursor != NULL; cursor = (const {node} *)cursor->tail)"));
                writer.line(&format!("if (length_{index} == UINT16_C({capacity})) {{ result.f0 = EMELA_PERIPHERAL_CAPACITY_EXCEEDED; return result; }}"));
                writer.line("if (cursor->head < 0 || cursor->head > INT32_C(65535)) { result.f0 = EMELA_PERIPHERAL_INVALID_ARGUMENT; return result; }");
                writer.line(&format!(
                    "input_{index}[length_{index}++] = (uint16_t)cursor->head;"
                ));
                writer.end_block();
                args.extend([format!("input_{index}"), format!("length_{index}")]);
            } else {
                args.push(to_board(layouts, ty, variable)?);
            }
        }
        let returns_list = int_list(&flow.core, op.ret);
        if returns_list {
            if op.name != "transfer" {
                let size = &variables[variables.len() - 2];
                writer.line(&format!("if ({size} < 0) {{ result.f0 = EMELA_PERIPHERAL_INVALID_ARGUMENT; return result; }}"));
                writer.line(&format!("if ((uint32_t){size} > UINT32_C({capacity})) {{ result.f0 = EMELA_PERIPHERAL_CAPACITY_EXCEEDED; return result; }}"));
            }
            writer.line(&format!("uint16_t output[{capacity}] = {{0}};"));
            writer.line("uint16_t output_length = 0;");
            args.extend([
                "output".into(),
                format!("UINT16_C({capacity})"),
                "&output_length".into(),
            ]);
        } else if op.fails && op.ret != TypeId::UNIT {
            args.push("&result.f1".into());
        }
        let call = format!("emela_{}_{}({})", op.peripheral, op.name, args.join(", "));
        if op.fails {
            writer.assign("result.f0", &call);
            if returns_list {
                writer.line("if (result.f0 != 0) return result;");
                if capacity != u16::MAX {
                    writer.line(&format!("if (output_length > UINT16_C({capacity})) {{ result.f0 = EMELA_PERIPHERAL_CAPACITY_EXCEEDED; return result; }}"));
                }
                if op.name != "transfer" {
                    let size = &variables[variables.len() - 2];
                    writer.line(&format!("if ((uint32_t)output_length > (uint32_t){size}) {{ result.f0 = EMELA_PERIPHERAL_CAPACITY_EXCEEDED; return result; }}"));
                } else {
                    let index = op
                        .params
                        .iter()
                        .position(|&ty| int_list(&flow.core, ty))
                        .unwrap();
                    writer.line(&format!("if (output_length > length_{index}) {{ result.f0 = EMELA_PERIPHERAL_CAPACITY_EXCEEDED; return result; }}"));
                }
                let node = layouts.list_node_name(op.ret);
                let kind = layouts.list_index(op.ret);
                writer.begin_block("for (uint16_t index = output_length; index-- > 0;)");
                writer.line(&format!("{node} *node = ({node} *)emela_heap_allocate(sizeof({node}), UINT32_C({kind}));"));
                writer.line("node->head = (int32_t)output[index];");
                writer.line("node->tail = result.f1;");
                writer.line("result.f1 = node;");
                writer.end_block();
            }
            writer.ret(Some("result"));
        } else if op.ret == TypeId::UNIT {
            writer.line(&format!("{call};"));
        } else {
            writer.ret(Some(&call));
        }
    }
    writer.end_block();
    writer.blank();
    Ok(true)
}
