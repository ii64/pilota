pub mod recursive_type {
    #![allow(warnings, clippy::all)]
    pub mod recursive_type {
        #[derive(PartialOrd, Hash, Eq, Ord, Debug, Default, Clone, PartialEq)]
        pub struct A {
            pub a: ::std::option::Option<::std::boxed::Box<A>>,
        }
        #[::async_trait::async_trait]
        impl ::pilota::thrift::Message for A {
            fn encode<T: ::pilota::thrift::TOutputProtocol>(
                &self,
                protocol: &mut T,
            ) -> ::std::result::Result<(), ::pilota::thrift::Error> {
                use ::pilota::thrift::TOutputProtocolExt;
                let struct_ident = ::pilota::thrift::TStructIdentifier { name: "A" };
                protocol.write_struct_begin(&struct_ident)?;
                if let Some(value) = self.a.as_ref() {
                    protocol.write_struct_field(1i16, value)?;
                };
                protocol.write_field_stop()?;
                protocol.write_struct_end()?;
                Ok(())
            }
            fn decode<T: ::pilota::thrift::TInputProtocol>(
                protocol: &mut T,
            ) -> ::std::result::Result<Self, ::pilota::thrift::Error> {
                let mut a = None;
                protocol.read_struct_begin()?;
                loop {
                    let field_ident = protocol.read_field_begin()?;
                    if field_ident.field_type == ::pilota::thrift::TType::Stop {
                        break;
                    }
                    let field_id = field_ident.id;
                    match field_id {
                        Some(1i16) if field_ident.field_type == ::pilota::thrift::TType::Struct => {
                            a = Some(::std::boxed::Box::new(::pilota::thrift::Message::decode(
                                protocol,
                            )?));
                        }
                        _ => {
                            protocol.skip(field_ident.field_type)?;
                        }
                    }
                    protocol.read_field_end()?;
                }
                protocol.read_struct_end()?;
                let data = Self { a };
                Ok(data)
            }
            async fn decode_async<T: ::pilota::thrift::TAsyncInputProtocol>(
                protocol: &mut T,
            ) -> ::std::result::Result<Self, ::pilota::thrift::Error> {
                let mut a = None;
                protocol.read_struct_begin().await?;
                loop {
                    let field_ident = protocol.read_field_begin().await?;
                    if field_ident.field_type == ::pilota::thrift::TType::Stop {
                        break;
                    }
                    let field_id = field_ident.id;
                    match field_id {
                        Some(1i16) if field_ident.field_type == ::pilota::thrift::TType::Struct => {
                            a = Some(::std::boxed::Box::new(
                                ::pilota::thrift::Message::decode_async(protocol).await?,
                            ));
                        }
                        _ => {
                            protocol.skip(field_ident.field_type).await?;
                        }
                    }
                    protocol.read_field_end().await?;
                }
                protocol.read_struct_end().await?;
                let data = Self { a };
                Ok(data)
            }
            fn size<T: ::pilota::thrift::TLengthProtocol>(&self, protocol: &mut T) -> usize {
                use ::pilota::thrift::TLengthProtocolExt;
                protocol.write_struct_begin_len(&::pilota::thrift::TStructIdentifier { name: "A" })
                    + self.a.as_ref().map_or(0, |value| {
                        protocol.write_struct_field_len(Some(1i16), value)
                    })
                    + protocol.write_field_stop_len()
                    + protocol.write_struct_end_len()
            }
        }
    }
}
