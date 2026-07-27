mod pg {
    pub fn get_db_string() -> String {
        std::env::var("DATABASE_URL").unwrap()
    }

    #[cfg(feature = "with-postgis-sqlx")]
    pub async fn get_pool() -> sqlx::Pool<sqlx::Postgres> {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&get_db_string())
            .await
            .unwrap()
    }
}

#[cfg(feature = "with-postgis-postgres")]
mod postgis_postgres {
    use geozero::wkt::WktWriter;
    use geozero::{ToWkt as _, wkb};

    use crate::pg::get_db_string;

    #[test]
    #[ignore]
    fn blob_query() -> Result<(), postgres::error::Error> {
        let mut client = postgres::Client::connect(&get_db_string(), postgres::NoTls).unwrap();
        let row = client.query_one(
            "SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry::bytea",
            &[],
        )?;
        let blob: &[u8] = row.get(0);
        let wkt = wkb::Ewkb(blob.to_vec()).to_wkt().expect("to_wkt failed");
        assert_eq!(&wkt, "POLYGON((0 0,2 0,2 2,0 2,0 0))");

        Ok(())
    }

    #[test]
    #[ignore]
    fn rust_geo_query() -> Result<(), postgres::error::Error> {
        let mut client = postgres::Client::connect(&get_db_string(), postgres::NoTls).unwrap();

        let row = client.query_one(
            "SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry",
            &[],
        )?;

        let value: wkb::Decode<geo_types::Geometry<f64>> = row.get(0);
        if let Some(geo_types::Geometry::Polygon(poly)) = value.geometry {
            assert_eq!(
                *poly.exterior(),
                vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 0.0)].into()
            );
        } else {
            panic!("Conversion to geo_types::Geometry failed");
        }

        let row = client.query_one("SELECT NULL::geometry", &[])?;
        let value: wkb::Decode<geo_types::Geometry<f64>> = row.get(0);
        assert!(value.geometry.is_none());
        let row = client.query_one("SELECT NULL::geometry", &[])?;
        let value: Result<wkb::Decode<geo_types::Geometry<f64>>, _> = row.try_get(0);
        assert!(value.unwrap().geometry.is_none());

        // Insert geometry
        let geom: geo_types::Geometry<f64> = geo::Point::new(1.0, 3.0).into();
        let _ = client.execute(
            "INSERT INTO point2d (datetimefield,geom) VALUES(now(),ST_SetSRID($1,4326))",
            &[&wkb::Encode(geom)],
        );

        Ok(())
    }

    #[test]
    #[ignore]
    #[cfg(feature = "with-geos")]
    fn geos_query() -> Result<(), postgres::error::Error> {
        let mut client = postgres::Client::connect(&get_db_string(), postgres::NoTls).unwrap();

        let row = client.query_one(
            "SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry",
            &[],
        )?;

        let value: wkb::Decode<geos::Geometry> = row.get(0);
        assert_eq!(
            value.geometry.unwrap().to_wkt().unwrap(),
            "POLYGON((0 0,2 0,2 2,0 2,0 0))"
        );

        // Insert geometry
        let geom = geos::Geometry::new_from_wkt("POINT(1 3)").expect("Invalid geometry");
        let _ = client.execute(
            "INSERT INTO point2d (datetimefield,geom) VALUES(now(),$1)",
            &[&wkb::Encode(geom)],
        );

        Ok(())
    }

    mod register_type {
        use postgres_types::{FromSql, Type};

        use super::*;

        struct Wkt(String);

        impl FromSql<'_> for Wkt {
            fn from_sql(
                _ty: &Type,
                raw: &[u8],
            ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
                let mut rdr = std::io::Cursor::new(raw);
                let mut wkt_data: Vec<u8> = Vec::new();
                let mut writer = WktWriter::new(&mut wkt_data);
                wkb::process_ewkb_geom(&mut rdr, &mut writer)?;
                let wkt = Wkt(std::str::from_utf8(&wkt_data)?.to_string());
                Ok(wkt)
            }

            fn accepts(ty: &Type) -> bool {
                matches!(ty.name(), "geography" | "geometry")
            }
        }

        #[test]
        #[ignore]
        fn geometry_query() -> Result<(), postgres::error::Error> {
            let mut client = postgres::Client::connect(&get_db_string(), postgres::NoTls).unwrap();

            let row = client.query_one(
                "SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry",
                &[],
            )?;

            let wkt_geom: Wkt = row.get(0);
            assert_eq!(&wkt_geom.0, "POLYGON((0 0,2 0,2 2,0 2,0 0))");
            Ok(())
        }
    }
}

#[cfg(feature = "with-postgis-sqlx")]
mod postgis_sqlx {
    use geozero::{ToWkt as _, wkb};

    use super::PointZ;
    use crate::pg;

    #[tokio::test]
    #[ignore]
    async fn blob_query() -> Result<(), sqlx::Error> {
        let pool = pg::get_pool().await;

        let row: (Vec<u8>,) = sqlx::query_as(
            "SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry::bytea",
        )
        .fetch_one(&pool)
        .await?;

        let wkt = wkb::Ewkb(row.0).to_wkt().expect("to_wkt failed");
        assert_eq!(&wkt, "POLYGON((0 0,2 0,2 2,0 2,0 0))");

        let row: (wkb::Ewkb<Vec<u8>>,) =
            sqlx::query_as("SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry")
                .fetch_one(&pool)
                .await?;

        let wkt = row.0.to_wkt().expect("to_wkt failed");
        assert_eq!(&wkt, "POLYGON((0 0,2 0,2 2,0 2,0 0))");

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn point3d_query() -> Result<(), sqlx::Error> {
        let pool = pg::get_pool().await;

        let row: (PointZ,) = sqlx::query_as("SELECT 'POINT(1 2 3)'::geometry")
            .fetch_one(&pool)
            .await?;

        let geom = row.0;
        assert_eq!(
            geom,
            PointZ {
                x: 1.0,
                y: 2.0,
                z: 3.0
            }
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn rust_geo_query() -> Result<(), sqlx::Error> {
        let pool = pg::get_pool().await;

        let row: (wkb::Decode<geo_types::Geometry<f64>>,) =
            sqlx::query_as("SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry")
                .fetch_one(&pool)
                .await?;

        let value = row.0;
        if let Some(geo_types::Geometry::Polygon(poly)) = value.geometry {
            assert_eq!(
                *poly.exterior(),
                vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0), (0.0, 0.0)].into()
            );
        } else {
            panic!("Conversion to geo_types::Geometry failed");
        }

        let row: (wkb::Decode<geo_types::Geometry<f64>>,) = sqlx::query_as("SELECT NULL::geometry")
            .fetch_one(&pool)
            .await?;
        assert!(row.0.geometry.is_none());

        // Insert geometry
        let geom: geo_types::Geometry<f64> = geo::Point::new(10.0, 20.0).into();
        let inserted = sqlx::query(
            "INSERT INTO point2d (datetimefield, geom) VALUES(now(), ST_SetSRID($1,4326))",
        )
        .bind(wkb::Encode(geom))
        .execute(&pool)
        .await?;

        assert_eq!(inserted.rows_affected(), 1);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn bulk_insert() -> Result<(), sqlx::Error> {
        // https://github.com/launchbadge/sqlx/blob/v0.5.13/FAQ.md#how-can-i-bind-an-array-to-a-values-clause-how-can-i-do-bulk-inserts
        let pool = pg::get_pool().await;

        let geom: geo_types::Geometry<f64> = geo::Point::new(10.0, 20.0).into();
        let geoms = [wkb::Encode(geom.clone()), wkb::Encode(geom.clone())];
        let inserted = sqlx::query(
            "INSERT INTO point2d (datetimefield, geom)
               SELECT now(), ST_SetSRID(g,4326) FROM UNNEST($1::geometry[]) as g",
        )
        .bind(&geoms[..])
        .execute(&pool)
        .await?;

        assert_eq!(inserted.rows_affected(), geoms.len() as u64);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    // Requires DATABASE_URL at compile time
    #[cfg(not(test))] // Delete this line to compile the test
    async fn rust_geo_macro_query() -> Result<(), sqlx::Error> {
        let pool = pg::get_pool().await;

        let mut tx = pool.begin().await?;

        let _ = sqlx::query!("DELETE FROM point2d").execute(&mut tx).await?;

        let rec = sqlx::query!("SELECT count(*) as count FROM point2d")
            .fetch_one(&mut tx)
            .await?;
        assert_eq!(rec.count, Some(0));

        let geom: geo_types::Geometry<f64> = geo::Point::new(10.0, 20.0).into();
        // https://docs.rs/sqlx/0.5.1/sqlx/macro.query.html?search=insert#type-overrides-bind-parameters-postgres-only
        let inserted = sqlx::query!(
            "INSERT INTO point2d (datetimefield, geom) VALUES(now(), ST_SetSRID($1::geometry,4326))",
            wkb::Encode(geom) as _
        )
        .execute(&mut tx)
        .await?;

        assert_eq!(inserted.rows_affected(), 1);

        // https://docs.rs/sqlx/0.5.1/sqlx/macro.query.html#force-a-differentcustom-type
        let rec = sqlx::query!(
            r#"SELECT datetimefield, geom as "geom!: wkb::Decode<geo_types::Geometry<f64>>" FROM point2d"#
        )
        .fetch_one(&mut tx)
        .await?;
        assert_eq!(
            rec.geom.geometry.unwrap(),
            geo::Point::new(10.0, 20.0).into()
        );

        struct PointRec {
            pub geom: wkb::Decode<geo_types::Geometry<f64>>,
            pub datetimefield: Option<sqlx::types::time::OffsetDateTime>,
        }
        let rec = sqlx::query_as!(
            PointRec,
            r#"SELECT datetimefield, geom as "geom!: _" FROM point2d"#
        )
        .fetch_one(&mut tx)
        .await?;
        assert_eq!(
            rec.geom.geometry.unwrap(),
            geo::Point::new(10.0, 20.0).into()
        );

        tx.rollback().await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    #[cfg(feature = "with-geos")]
    async fn geos_query() -> Result<(), sqlx::Error> {
        let pool = pg::get_pool().await;

        let row: (wkb::Decode<geos::Geometry>,) =
            sqlx::query_as("SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry")
                .fetch_one(&pool)
                .await?;
        let value = row.0;
        assert_eq!(
            value.geometry.unwrap().to_wkt().unwrap(),
            "POLYGON((0 0,2 0,2 2,0 2,0 0))"
        );

        let row: (wkb::Decode<geos::Geometry>,) = sqlx::query_as("SELECT NULL::geometry")
            .fetch_one(&pool)
            .await?;
        let value = row.0;
        assert!(value.geometry.is_none());

        // Insert geometry
        let geom = geos::Geometry::new_from_wkt("POINT(1 3)").expect("Invalid geometry");
        let inserted = sqlx::query("INSERT INTO point2d (datetimefield,geom) VALUES(now(),$1)")
            .bind(wkb::Encode(geom))
            .execute(&pool)
            .await?;

        assert_eq!(inserted.rows_affected(), 1);

        Ok(())
    }

    mod register_type {
        use geozero::wkt::WktWriter;
        use sqlx::ValueRef;
        use sqlx::decode::Decode;
        use sqlx::postgres::{PgTypeInfo, PgValueRef, Postgres};

        use super::*;

        type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

        struct Text(String);

        impl sqlx::Type<Postgres> for Text {
            fn type_info() -> PgTypeInfo {
                PgTypeInfo::with_name("geometry")
            }
        }

        impl Decode<'_, Postgres> for Text {
            fn decode(value: PgValueRef) -> Result<Self, BoxDynError> {
                if value.is_null() {
                    return Ok(Text("EMPTY".to_string()));
                }
                let mut blob = <&[u8] as Decode<Postgres>>::decode(value)?;
                let mut data: Vec<u8> = Vec::new();
                let mut writer = WktWriter::new(&mut data);
                wkb::process_ewkb_geom(&mut blob, &mut writer)
                    .map_err(|e| sqlx::Error::Decode(e.to_string().into()))?;
                let text = Text(std::str::from_utf8(&data).unwrap().to_string());
                Ok(text)
            }
        }

        #[tokio::test]
        #[ignore]
        async fn geometry_query() -> Result<(), sqlx::Error> {
            let pool = pg::get_pool().await;

            let row: (Text,) =
                sqlx::query_as("SELECT 'SRID=4326;POLYGON ((0 0, 2 0, 2 2, 0 2, 0 0))'::geometry")
                    .fetch_one(&pool)
                    .await?;
            assert_eq!((row.0).0, "POLYGON((0 0,2 0,2 2,0 2,0 0))");

            let row: (Text,) = sqlx::query_as("SELECT NULL::geometry")
                .fetch_one(&pool)
                .await?;
            assert_eq!((row.0).0, "EMPTY");

            Ok(())
        }
    }
}

#[cfg(feature = "with-postgis-diesel")]
mod postgis_diesel {
    use postgis_diesel::types::{
        AnyPoint, GeometryCollection, GeometryContainer, LineString, MultiLineString, MultiPoint,
        MultiPolygon, Point, PointM, PointT, PointZ, PointZM, Polygon,
    };

    use geozero::wkb::{self, FromWkb};
    use geozero::{CoordDimensions, GeozeroGeometry, ToWkb, ToWkt};

    const SRID: Option<u32> = Some(4326);

    fn points(coords: &[(f64, f64)]) -> Vec<Point> {
        coords
            .iter()
            .map(|&(x, y)| Point::new(x, y, SRID))
            .collect()
    }

    fn line_string(coords: &[(f64, f64)]) -> LineString<Point> {
        LineString {
            points: points(coords),
            srid: SRID,
        }
    }

    fn polygon(rings: &[&[(f64, f64)]]) -> Polygon<Point> {
        Polygon {
            rings: rings.iter().map(|ring| points(ring)).collect(),
            srid: SRID,
        }
    }

    const RING: &[(f64, f64)] = &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)];

    /// `CoordDimensions` is neither `Debug` nor `PartialEq`, so compare the flags we care about.
    fn zm(dims: CoordDimensions) -> (bool, bool) {
        (dims.z, dims.m)
    }

    /// Encodes via [`GeozeroGeometry`] and decodes via [`FromWkb`], exercising both directions.
    fn roundtrip<G, P>(geom: &G) -> GeometryContainer<P>
    where
        G: GeozeroGeometry,
        P: PointT,
        GeometryContainer<P>: FromWkb,
    {
        let ewkb = geom.to_ewkb(geom.dims(), geom.srid()).unwrap();
        GeometryContainer::<P>::from_wkb(&mut ewkb.as_slice(), wkb::WkbDialect::Ewkb).unwrap()
    }

    #[test]
    fn geometries_to_wkt() {
        assert_eq!(Point::new(1.0, 2.0, SRID).to_wkt().unwrap(), "POINT(1 2)");
        assert_eq!(
            line_string(&[(0.0, 0.0), (1.0, 1.0)]).to_wkt().unwrap(),
            "LINESTRING(0 0,1 1)"
        );
        assert_eq!(
            polygon(&[RING]).to_wkt().unwrap(),
            "POLYGON((0 0,1 0,1 1,0 0))"
        );

        let multi_point = MultiPoint {
            points: points(&[(1.0, 2.0), (3.0, 4.0)]),
            srid: SRID,
        };
        assert_eq!(multi_point.to_wkt().unwrap(), "MULTIPOINT(1 2,3 4)");

        let multi_line_string = MultiLineString {
            lines: vec![
                line_string(&[(0.0, 0.0), (1.0, 1.0)]),
                line_string(&[(2.0, 2.0), (3.0, 3.0)]),
            ],
            srid: SRID,
        };
        assert_eq!(
            multi_line_string.to_wkt().unwrap(),
            "MULTILINESTRING((0 0,1 1),(2 2,3 3))"
        );

        let multi_polygon = MultiPolygon {
            polygons: vec![polygon(&[RING])],
            srid: SRID,
        };
        assert_eq!(
            multi_polygon.to_wkt().unwrap(),
            "MULTIPOLYGON(((0 0,1 0,1 1,0 0)))"
        );

        let collection = GeometryCollection {
            geometries: vec![
                GeometryContainer::Point(Point::new(1.0, 2.0, SRID)),
                GeometryContainer::LineString(line_string(&[(0.0, 0.0), (1.0, 1.0)])),
            ],
            srid: SRID,
        };
        assert_eq!(
            collection.to_wkt().unwrap(),
            "GEOMETRYCOLLECTION(POINT(1 2),LINESTRING(0 0,1 1))"
        );
    }

    #[test]
    fn polygon_rings_to_wkt() {
        let poly = polygon(&[
            &[
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ],
            &[(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0), (1.0, 1.0)],
        ]);
        assert_eq!(
            poly.to_wkt().unwrap(),
            "POLYGON((0 0,10 0,10 10,0 10,0 0),(1 1,2 1,2 2,1 2,1 1))"
        );
    }

    #[test]
    fn coordinate_dimensions_to_wkt() {
        let xym = CoordDimensions {
            z: false,
            m: true,
            t: false,
            tm: false,
        };

        assert_eq!(Point::new(1.0, 2.0, SRID).to_wkt().unwrap(), "POINT(1 2)");
        assert_eq!(
            PointZ::new(1.0, 2.0, 3.0, SRID)
                .to_wkt_ndim(CoordDimensions::xyz())
                .unwrap(),
            "POINT(1 2 3)"
        );
        // An M value is emitted as the third coordinate when only the M dimension is enabled
        assert_eq!(
            PointM::new(1.0, 2.0, 3.0, SRID).to_wkt_ndim(xym).unwrap(),
            "POINT(1 2 3)"
        );
        assert_eq!(
            PointZM::new(1.0, 2.0, 3.0, 4.0, SRID)
                .to_wkt_ndim(CoordDimensions::xyzm())
                .unwrap(),
            "POINT(1 2 3 4)"
        );
        assert_eq!(
            AnyPoint::PointZ(PointZ::new(1.0, 2.0, 3.0, SRID))
                .to_wkt_ndim(CoordDimensions::xyz())
                .unwrap(),
            "POINT(1 2 3)"
        );

        let line = LineString {
            points: vec![
                PointZ::new(0.0, 0.0, 10.0, SRID),
                PointZ::new(1.0, 1.0, 20.0, SRID),
            ],
            srid: SRID,
        };
        assert_eq!(
            line.to_wkt_ndim(CoordDimensions::xyz()).unwrap(),
            "LINESTRING(0 0 10,1 1 20)"
        );
    }

    #[test]
    fn dims_and_srid_are_reported() {
        let point = Point::new(1.0, 2.0, SRID);
        assert_eq!(GeozeroGeometry::srid(&point), Some(4326));
        assert_eq!(zm(GeozeroGeometry::dims(&point)), (false, false));

        let point_z = PointZ::new(1.0, 2.0, 3.0, Some(7415));
        assert_eq!(GeozeroGeometry::srid(&point_z), Some(7415));
        assert_eq!(zm(GeozeroGeometry::dims(&point_z)), (true, false));

        let measured = PointM::new(1.0, 2.0, 3.0, SRID);
        assert_eq!(zm(GeozeroGeometry::dims(&measured)), (false, true));

        let measured_3d = PointZM::new(1.0, 2.0, 3.0, 4.0, SRID);
        assert_eq!(zm(GeozeroGeometry::dims(&measured_3d)), (true, true));

        // Containers take their dimensions from their first point
        let line = line_string(&[(0.0, 0.0), (1.0, 1.0)]);
        assert_eq!(GeozeroGeometry::srid(&line), Some(4326));
        assert_eq!(zm(GeozeroGeometry::dims(&line)), (false, false));

        // ...and report XY when they are empty
        let empty = LineString::<PointZ> {
            points: Vec::new(),
            srid: SRID,
        };
        assert_eq!(zm(GeozeroGeometry::dims(&empty)), (false, false));
    }

    #[test]
    fn geometry_container_delegates_to_inner_geometry() {
        let container = GeometryContainer::Point(PointZ::new(1.0, 2.0, 3.0, SRID));
        assert_eq!(
            container.to_wkt_ndim(CoordDimensions::xyz()).unwrap(),
            "POINT(1 2 3)"
        );
        assert_eq!(GeozeroGeometry::srid(&container), Some(4326));
        assert_eq!(zm(GeozeroGeometry::dims(&container)), (true, false));

        let container = GeometryContainer::Polygon(polygon(&[RING]));
        assert_eq!(container.to_wkt().unwrap(), "POLYGON((0 0,1 0,1 1,0 0))");
    }

    #[test]
    fn geometries_from_ewkb() {
        let point = Point::new(1.0, 2.0, SRID);
        match roundtrip::<_, Point>(&point) {
            GeometryContainer::Point(p) => {
                assert_eq!((p.x, p.y, p.srid), (1.0, 2.0, SRID));
            }
            other => panic!("Expected Point, got {other:?}"),
        }

        let line = line_string(&[(0.0, 0.0), (1.0, 1.0), (2.0, 3.0)]);
        match roundtrip::<_, Point>(&line) {
            GeometryContainer::LineString(result) => {
                assert_eq!(result.points.len(), 3);
                assert_eq!((result.points[2].x, result.points[2].y), (2.0, 3.0));
                assert_eq!(result.srid, SRID);
            }
            other => panic!("Expected LineString, got {other:?}"),
        }

        let poly = polygon(&[RING]);
        match roundtrip::<_, Point>(&poly) {
            GeometryContainer::Polygon(result) => {
                assert_eq!(result.rings.len(), 1);
                assert_eq!(result.rings[0].len(), 4);
                assert_eq!(result.srid, SRID);
            }
            other => panic!("Expected Polygon, got {other:?}"),
        }

        let multi_point = MultiPoint {
            points: points(&[(1.0, 2.0), (3.0, 4.0)]),
            srid: SRID,
        };
        match roundtrip::<_, Point>(&multi_point) {
            GeometryContainer::MultiPoint(result) => {
                assert_eq!(result.points.len(), 2);
                assert_eq!((result.points[0].x, result.points[1].x), (1.0, 3.0));
                assert_eq!(result.srid, SRID);
            }
            other => panic!("Expected MultiPoint, got {other:?}"),
        }

        let multi_line_string = MultiLineString {
            lines: vec![
                line_string(&[(0.0, 0.0), (1.0, 1.0)]),
                line_string(&[(2.0, 2.0), (3.0, 3.0)]),
            ],
            srid: SRID,
        };
        match roundtrip::<_, Point>(&multi_line_string) {
            GeometryContainer::MultiLineString(result) => {
                assert_eq!(result.lines.len(), 2);
                assert_eq!(result.lines[1].points[0].x, 2.0);
                assert_eq!(result.srid, SRID);
            }
            other => panic!("Expected MultiLineString, got {other:?}"),
        }

        let multi_polygon = MultiPolygon {
            polygons: vec![polygon(&[RING])],
            srid: SRID,
        };
        match roundtrip::<_, Point>(&multi_polygon) {
            GeometryContainer::MultiPolygon(result) => {
                assert_eq!(result.polygons.len(), 1);
                assert_eq!(result.polygons[0].rings[0].len(), 4);
                assert_eq!(result.srid, SRID);
            }
            other => panic!("Expected MultiPolygon, got {other:?}"),
        }
    }

    #[test]
    fn geometry_collection_from_ewkb() {
        let collection = GeometryContainer::GeometryCollection(GeometryCollection {
            geometries: vec![
                GeometryContainer::Point(Point::new(1.0, 2.0, SRID)),
                GeometryContainer::LineString(line_string(&[(0.0, 0.0), (1.0, 1.0)])),
            ],
            srid: SRID,
        });

        match roundtrip::<_, Point>(&collection) {
            GeometryContainer::GeometryCollection(result) => {
                assert_eq!(result.srid, SRID);
                assert!(matches!(
                    result.geometries[..],
                    [
                        GeometryContainer::Point(_),
                        GeometryContainer::LineString(_)
                    ]
                ));
            }
            other => panic!("Expected GeometryCollection, got {other:?}"),
        }
    }

    #[test]
    fn coordinate_dimensions_from_ewkb() {
        match roundtrip::<_, PointZ>(&PointZ::new(1.0, 2.0, 3.0, SRID)) {
            GeometryContainer::Point(p) => {
                assert_eq!((p.x, p.y, p.z, p.srid), (1.0, 2.0, 3.0, SRID))
            }
            other => panic!("Expected PointZ, got {other:?}"),
        }

        match roundtrip::<_, PointM>(&PointM::new(1.0, 2.0, 3.0, SRID)) {
            GeometryContainer::Point(p) => {
                assert_eq!((p.x, p.y, p.m, p.srid), (1.0, 2.0, 3.0, SRID))
            }
            other => panic!("Expected PointM, got {other:?}"),
        }

        match roundtrip::<_, PointZM>(&PointZM::new(1.0, 2.0, 3.0, 4.0, SRID)) {
            GeometryContainer::Point(p) => {
                assert_eq!((p.x, p.y, p.z, p.m, p.srid), (1.0, 2.0, 3.0, 4.0, SRID));
            }
            other => panic!("Expected PointZM, got {other:?}"),
        }

        let any = AnyPoint::PointZ(PointZ::new(1.0, 2.0, 3.0, SRID));
        match roundtrip::<_, AnyPoint>(&any) {
            GeometryContainer::Point(AnyPoint::PointZ(p)) => {
                assert_eq!((p.x, p.y, p.z, p.srid), (1.0, 2.0, 3.0, SRID));
            }
            other => panic!("Expected AnyPoint::PointZ, got {other:?}"),
        }
    }

    #[cfg(feature = "with-geojson")]
    mod geojson {
        use serde_json::Value;

        use super::*;
        use geozero::ToJson;
        use geozero::geojson::GeoJsonString;

        /// The impls have to drive any [`geozero::GeomProcessor`], not just the WKT and WKB writers.
        #[test]
        fn geometries_to_geojson() {
            let json: Value = serde_json::from_str(&polygon(&[RING]).to_json().unwrap()).unwrap();
            assert_eq!(json["type"], "Polygon");
            assert_eq!(json["coordinates"][0][2][0], 1.0);

            let point = PointZ::new(1.0, 2.0, 3.0, SRID);
            let json: Value = serde_json::from_str(&point.to_json().unwrap()).unwrap();
            assert_eq!(json["type"], "Point");
            assert_eq!(json["coordinates"][2], 3.0);
        }

        /// [`FromWkb`] has to accept EWKB produced from any source, not just from postgis_diesel types.
        #[test]
        fn geojson_to_postgis_diesel() {
            let geojson =
                GeoJsonString(r#"{"type":"Point","coordinates":[1.0,2.0,3.0]}"#.to_string());
            let ewkb = geojson.to_ewkb(CoordDimensions::xyz(), None).unwrap();
            let container = GeometryContainer::<AnyPoint>::from_wkb(
                &mut ewkb.as_slice(),
                wkb::WkbDialect::Ewkb,
            )
            .unwrap();
            match container {
                GeometryContainer::Point(AnyPoint::PointZ(p)) => {
                    assert_eq!((p.x, p.y, p.z), (1.0, 2.0, 3.0));
                }
                other => panic!("Expected AnyPoint::PointZ, got {other:?}"),
            }

            let geojson = GeoJsonString(
                r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]],[[[2,2],[3,2],[3,3],[2,2]]]]}"#
                    .to_string(),
            );
            let ewkb = geojson.to_ewkb(CoordDimensions::xy(), None).unwrap();
            let container =
                GeometryContainer::<Point>::from_wkb(&mut ewkb.as_slice(), wkb::WkbDialect::Ewkb)
                    .unwrap();
            match container {
                GeometryContainer::MultiPolygon(mp) => {
                    assert_eq!(mp.polygons.len(), 2);
                    assert_eq!(mp.polygons[0].rings[0][0].x, 0.0);
                    assert_eq!(mp.polygons[1].rings[0][0].x, 2.0);
                }
                other => panic!("Expected MultiPolygon, got {other:?}"),
            }
        }
    }
}

// --- Minimal geometry implementation with PostGIS/GPKG support

use std::io::Read;

use geozero::wkb::{FromWkb, WkbDialect};
use geozero::{CoordDimensions, GeomProcessor, GeozeroGeometry};

#[derive(Debug, PartialEq, Default)]
struct PointZ {
    x: f64,
    y: f64,
    z: f64,
}

impl GeomProcessor for PointZ {
    fn dimensions(&self) -> CoordDimensions {
        CoordDimensions::xyz()
    }
    fn coordinate(
        &mut self,
        x: f64,
        y: f64,
        z: Option<f64>,
        _m: Option<f64>,
        _t: Option<f64>,
        _tm: Option<u64>,
        _idx: usize,
    ) -> geozero::error::Result<()> {
        self.x = x;
        self.y = y;
        self.z = z.unwrap_or(0.0);
        Ok(())
    }
}

impl GeozeroGeometry for PointZ {
    fn process_geom<P: GeomProcessor>(
        &self,
        processor: &mut P,
    ) -> Result<(), geozero::error::GeozeroError> {
        processor.point_begin(0)?;
        processor.coordinate(self.x, self.y, Some(self.z), None, None, None, 0)?;
        processor.point_end(0)
    }
    fn dims(&self) -> CoordDimensions {
        CoordDimensions::xyz()
    }
}

impl FromWkb for PointZ {
    fn from_wkb<R: Read>(rdr: &mut R, dialect: WkbDialect) -> geozero::error::Result<Self> {
        let mut pt = PointZ::default();
        geozero::wkb::process_wkb_type_geom(rdr, &mut pt, dialect)?;
        Ok(pt)
    }
}

#[cfg(feature = "with-postgis-postgres")]
mod postgis_postgres_macros {
    geozero::impl_postgres_postgis_decode!(super::PointZ);
    geozero::impl_postgres_postgis_encode!(super::PointZ);
}
#[cfg(feature = "with-postgis-sqlx")]
mod postgis_sqlx_macros {
    geozero::impl_sqlx_postgis_type_info!(super::PointZ);
    geozero::impl_sqlx_postgis_decode!(super::PointZ);
    geozero::impl_sqlx_postgis_encode!(super::PointZ);
}
#[cfg(feature = "with-postgis-diesel")]
mod postgis_diesel_macros {
    geozero::impl_diesel_postgis_decode!(super::PointZ);
    geozero::impl_diesel_postgis_encode!(super::PointZ);
}
#[cfg(feature = "with-gpkg")]
mod gpkg_sqlx_macros {
    geozero::impl_sqlx_gpkg_type_info!(super::PointZ);
    geozero::impl_sqlx_gpkg_decode!(super::PointZ);
    geozero::impl_sqlx_gpkg_encode!(super::PointZ);
}
