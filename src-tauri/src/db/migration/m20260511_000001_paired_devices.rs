use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PairedDevice::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PairedDevice::DeviceId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PairedDevice::Nickname).string().not_null())
                    .col(
                        ColumnDef::new(PairedDevice::ClientPubHex)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(PairedDevice::RelaySessionId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PairedDevice::PairedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PairedDevice::LastActiveAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PairedDevice::Revoked)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_paired_devices_last_active_at")
                    .table(PairedDevice::Table)
                    .col(PairedDevice::LastActiveAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PairedDevice::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PairedDevice {
    #[sea_orm(iden = "paired_devices")]
    Table,
    DeviceId,
    Nickname,
    ClientPubHex,
    RelaySessionId,
    PairedAt,
    LastActiveAt,
    Revoked,
}
