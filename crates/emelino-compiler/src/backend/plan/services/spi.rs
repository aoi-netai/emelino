//! Validate the concrete shapes assumed by the public compound-SPI C ABI.
//! Saved IR can change both an effect and its private binding consistently;
//! matching those signatures alone does not establish the supported ABI.
use super::*;

fn definition<'a>(core: &'a Definitions, ty: TypeId, name: &str) -> Option<&'a EnumDef> {
    let Type::Enum(id, arguments) = &core.types[ty] else {
        return None;
    };
    let def = &core.enums[*id];
    (arguments.is_empty() && def.name == name && def.params == 0 && !def.error && !def.external)
        .then_some(def)
}

fn record<'a>(
    core: &'a Definitions,
    ty: TypeId,
    name: &str,
    fields: &[&str],
) -> Option<&'a [TypeId]> {
    let def = definition(core, ty, name)?;
    let [variant] = def.variants.as_slice() else {
        return None;
    };
    (variant.name == name.rsplit('.').next().unwrap()
        && matches!(&variant.fields, VariantFields::Named(names) if names == fields)
        && variant.tys.len() == fields.len())
    .then_some(variant.tys.as_slice())
}

fn scalar(core: &Definitions, ty: TypeId, name: &str, variants: &[&str]) -> bool {
    definition(core, ty, name).is_some_and(|def| {
        def.variants.len() == variants.len()
            && def
                .variants
                .iter()
                .zip(variants)
                .all(|(variant, name)| variant.name == *name && variant.tys.is_empty())
    })
}

fn words(core: &Definitions, ty: TypeId) -> bool {
    matches!(core.types[ty], Type::List(element) if element == TypeId::INT)
}

fn device(core: &Definitions, ty: TypeId) -> bool {
    record(core, ty, "Embedded.Spi.SpiDevice", &["slot"]) == Some(&[TypeId::INT][..])
}

fn bus(core: &Definitions, ty: TypeId) -> bool {
    scalar(
        core,
        ty,
        "Embedded.Spi.SpiId",
        &["Spi1", "Spi2", "Spi3", "Spi4", "Spi5", "Spi6"],
    )
}

fn config(core: &Definitions, ty: TypeId) -> bool {
    let Some(fields) = record(
        core,
        ty,
        "Embedded.Spi.SpiConfig",
        &[
            "mode",
            "direction",
            "data_size",
            "polarity",
            "phase",
            "max_frequency_hz",
            "bit_order",
        ],
    ) else {
        return false;
    };
    scalar(
        core,
        fields[0],
        "Embedded.Spi.SpiMode",
        &["Master", "Slave"],
    ) && scalar(
        core,
        fields[1],
        "Embedded.Spi.SpiDirection",
        &["TwoLine", "TwoLineReceiveOnly", "OneLine"],
    ) && scalar(
        core,
        fields[2],
        "Embedded.Spi.SpiDataSize",
        &["Bits8", "Bits16"],
    ) && scalar(
        core,
        fields[3],
        "Embedded.Spi.ClockPolarity",
        &["IdleLow", "IdleHigh"],
    ) && scalar(
        core,
        fields[4],
        "Embedded.Spi.ClockPhase",
        &["FirstEdge", "SecondEdge"],
    ) && fields[5] == TypeId::INT
        && scalar(
            core,
            fields[6],
            "Embedded.Spi.BitOrder",
            &["MostSignificantFirst", "LeastSignificantFirst"],
        )
}

fn device_config(core: &Definitions, ty: TypeId) -> bool {
    let Some(fields) = record(
        core,
        ty,
        "Embedded.Spi.SpiDeviceConfig",
        &["bus", "config", "cs", "active", "setup_us", "hold_us"],
    ) else {
        return false;
    };
    bus(core, fields[0])
        && config(core, fields[1])
        && record(core, fields[2], "Embedded.Gpio.GpioPin", &["id"]) == Some(&[TypeId::INT][..])
        && scalar(core, fields[3], "Embedded.Gpio.PinState", &["Low", "High"])
        && fields[4..] == [TypeId::INT64, TypeId::INT64]
}

fn operations(core: &Definitions, ty: TypeId) -> bool {
    let Type::List(element) = core.types[ty] else {
        return false;
    };
    let Some(def) = definition(core, element, "Embedded.Spi.SpiOperation") else {
        return false;
    };
    def.variants.len() == 3
        && def
            .variants
            .iter()
            .zip(["Write", "Read", "Transfer"])
            .all(|(variant, name)| {
                let [field] = variant.tys.as_slice() else {
                    return false;
                };
                variant.name == name
                    && if name == "Read" {
                        *field == TypeId::INT
                    } else {
                        words(core, *field)
                    }
            })
}

pub(super) fn valid(core: &Definitions, operation: &Operation) -> bool {
    if operation.peripheral != "spi" {
        return true;
    }
    match operation.name.as_str() {
        "create_device" => {
            matches!(operation.params.as_slice(), [config] if device_config(core, *config))
                && device(core, operation.ret)
                && operation.fails
        }
        "transaction" => {
            matches!(operation.params.as_slice(), [handle, ops, timeout]
                if device(core, *handle) && operations(core, *ops) && *timeout == TypeId::INT64)
                && matches!(core.types[operation.ret], Type::List(inner) if words(core, inner))
                && operation.fails
        }
        "recover" => {
            matches!(operation.params.as_slice(), [id] if bus(core, *id))
                && operation.ret == TypeId::UNIT
                && operation.fails
        }
        _ => true,
    }
}
