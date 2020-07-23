mod models {
    #[async_graphql::SimpleObject]
    #[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
    pub struct Contact {
        pub id: String,
        pub first_name: String,
        pub last_name: String,
    }

    impl Contact {
        pub fn new(id: &str, first_name: &str, last_name: &str) -> Self {
            Self {
                id: id.to_owned(),
                first_name: first_name.to_owned(),
                last_name: last_name.to_owned(),
            }
        }
    }
}

mod usecases {

    pub struct Contacts {}

    impl Contacts {
        pub fn create(
            contact: crate::models::Contact,
            repo: &impl crate::repo::Repository<crate::models::Contact>,
        ) -> Result<crate::models::Contact, Box<dyn std::error::Error>> {
            let r = repo.set(contact.clone());
            println!("contact created {:?}", contact);
            r
        }
    }
}

mod repo {
    pub trait Repository<T> {
        fn set(&self, obj: T) -> Result<T, Box<dyn std::error::Error>>;
        fn get(&self, id: &str) -> Result<T, Box<dyn std::error::Error>>;
    }

    #[derive(Clone)]
    pub struct FileRepository {
        path: String,
    }

    impl FileRepository {
        pub fn new(path: &str) -> FileRepository {
            FileRepository {
                path: path.to_owned(),
            }
        }
    }

    impl<T: serde::de::DeserializeOwned + serde::Serialize> crate::repo::Repository<T>
        for FileRepository
    {
        fn set(&self, obj: T) -> Result<T, Box<dyn std::error::Error>> {
            use std::fs::File;
            let f = File::create(&self.path)?;
            serde_json::to_writer(f, &obj).expect("Unable to serialized");
            Ok(obj)
        }

        fn get(&self, id: &str) -> Result<T, Box<dyn std::error::Error>> {
            use std::fs::File;
            let f = File::open(&self.path)?;
            let result: T = serde_json::from_reader(f).expect("Unable to serialized");
            Ok(result)
        }
    }
}

mod graphql {

    use actix_web::{guard, web, App, HttpResponse, HttpServer};
    use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
    use async_graphql::{Context, EmptySubscription, IntoQueryBuilderOpts, Schema};
    use async_graphql_actix_web::{GQLRequest, GQLResponse};

    type ContactsSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

    async fn index(schema: web::Data<ContactsSchema>, req: GQLRequest) -> GQLResponse {
        req.into_inner().execute(&schema).await.into()
    }

    async fn gql_playgound() -> HttpResponse {
        HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(playground_source(GraphQLPlaygroundConfig::new("/")))
    }

    struct QueryRoot;

    #[async_graphql::Object]
    impl QueryRoot {
        async fn get(
            &self,
            _: &Context<'_>,
            #[arg(desc = "id")] id: String,
            #[arg(desc = "firstname")] first_name: String,
            #[arg(desc = "lastname")] last_name: String,
        ) -> crate::models::Contact {
            crate::models::Contact::new(id.as_str(), first_name.as_str(), last_name.as_str())
        }
    }

    #[async_graphql::SimpleObject]
    struct QueryContact {
        first_name: String,
        last_name: String,
    }

    impl std::convert::From<crate::models::Contact> for QueryContact {
        fn from(c: crate::models::Contact) -> Self {
            Self {
                first_name: c.first_name.to_owned(),
                last_name: c.last_name.to_owned(),
            }
        }
    }

    struct MutationRoot;

    #[async_graphql::Object]
    impl MutationRoot {
        async fn create(
            &self,
            ctx: &Context<'_>,
            #[arg(desc = "contact")] contact: MutationCreate,
        ) -> Result<QueryContact, async_graphql::FieldError> {
            let repo = ctx.data_unchecked::<crate::repo::FileRepository>();
            let model: crate::models::Contact = contact.into();
            crate::usecases::Contacts::create(model, &repo.clone()).map_or_else(
                |_| Err(async_graphql::FieldError("oops".to_owned(), None)),
                |c| Ok(QueryContact::from(c)),
            )
        }
    }

    #[async_graphql::InputObject]
    struct MutationCreate {
        id: String,
        first_name: String,
        last_name: String,
    }

    impl std::convert::From<crate::models::Contact> for MutationCreate {
        fn from(c: crate::models::Contact) -> Self {
            Self {
                id: c.id.to_owned(),
                first_name: c.first_name.to_owned(),
                last_name: c.last_name.to_owned(),
            }
        }
    }

    impl std::convert::Into<crate::models::Contact> for MutationCreate {
        fn into(self) -> crate::models::Contact {
            crate::models::Contact {
                id: self.id.to_owned(),
                first_name: self.first_name.to_owned(),
                last_name: self.last_name.to_owned(),
            }
        }
    }

    pub async fn start_server() -> std::io::Result<()> {
        let repo = crate::repo::FileRepository::new("/tmp/foo.json");
        let local = tokio::task::LocalSet::new();
        let sys = actix_rt::System::run_in_tokio("server", &local);

        let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
            .data(repo.clone())
            .finish();

        println!("Playground: http://localhost:8000");

        let server_res = HttpServer::new(move || {
            App::new()
                .data(schema.clone())
                .service(web::resource("/").guard(guard::Post()).to(index).app_data(
                    IntoQueryBuilderOpts {
                        max_num_files: Some(3),
                        ..IntoQueryBuilderOpts::default()
                    },
                ))
                .service(web::resource("/").guard(guard::Get()).to(gql_playgound))
        })
        .bind("127.0.0.1:8000")?
        .run()
        .await?;
        sys.await?;
        Ok(server_res)
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    graphql::start_server().await?;
    Ok(())
}
