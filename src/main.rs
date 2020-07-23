#[macro_use]
extern crate log;

mod models {
    use async_graphql::SimpleObject;
    use serde::{Deserialize, Serialize};

    #[SimpleObject]
    #[derive(Debug, Serialize, Deserialize, Clone, Hash)]
    pub struct Contact {
        pub id: String,
        pub first_name: String,
        pub last_name: String,
    }
}

mod usecases {

    use super::models::*;
    use super::repo::*;
    use std::error::Error;

    pub fn create(
        contact: Contact,
        repo: &dyn Repository<Contact>,
    ) -> Result<Contact, Box<dyn Error>> {
        let r = repo.set(contact.clone())?;
        println!("contact created {:?}", contact);
        Ok(r)
    }

    pub fn get(id: &str, repo: &dyn Repository<Contact>) -> Result<Contact, Box<dyn Error>> {
        repo.get(id)
    }
}

mod repo {
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use std::error::Error;
    use std::hash::Hash;

    pub trait Repository<T> {
        fn set(&self, obj: T) -> Result<T, Box<dyn Error>>;
        fn get(&self, id: &str) -> Result<T, Box<dyn Error>>;
    }

    pub struct FileRepository<'a> {
        path: &'a str,
    }

    impl<'a> FileRepository<'a> {
        pub fn new(path: &'a str) -> FileRepository<'a> {
            FileRepository { path }
        }
    }

    impl<'a, T: DeserializeOwned + Serialize + Hash> Repository<T> for FileRepository<'a> {
        fn set(&self, obj: T) -> Result<T, Box<dyn Error>> {
            use std::collections::hash_map::DefaultHasher;
            use std::fs::File;
            use std::hash::Hasher;
            use std::path::Path;

            let mut hasher = DefaultHasher::new();
            obj.hash(&mut hasher);
            let hash = hasher.finish();
            let path = Path::new(&self.path).join(format!("{}.json", hash));
            println!("{:?}", path);

            let f = File::create(path)?;
            serde_json::to_writer(f, &obj).expect("Unable to serialized");
            Ok(obj)
        }

        fn get(&self, id: &str) -> Result<T, Box<dyn Error>> {
            use std::fs::File;
            use std::path::Path;
            let path = Path::new(&self.path).join(format!("{}.json", id));
            println!("{:?}", path);
            let f = File::open(&path)?;
            let result: T = serde_json::from_reader(f).expect("Unable to serialized");
            Ok(result)
        }
    }
}

mod graphql {

    use super::models::*;
    use super::repo::*;
    use super::usecases::*;
    use actix_web::{guard, web, App, HttpResponse, HttpServer};
    use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
    use async_graphql::*;
    use async_graphql_actix_web::{GQLRequest, GQLResponse};

    type ContactsSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

    async fn index(schema: web::Data<ContactsSchema>, req: GQLRequest) -> GQLResponse {
        debug!("request");
        req.into_inner().execute(&schema).await.into()
    }

    async fn gql_playgound() -> HttpResponse {
        debug!("playground");
        HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(playground_source(GraphQLPlaygroundConfig::new("/")))
    }

    struct QueryRoot;

    #[Object]
    impl QueryRoot {
        async fn get(
            &self,
            ctx: &Context<'_>,
            #[arg(desc = "id")] id: String,
        ) -> FieldResult<Contact> {
            let repo = ctx.data_unchecked::<FileRepository>();
            match get(id.as_str(), repo) {
                Ok(c) => Ok(c),
                Err(e) => Err(FieldError(format!("{}", e).to_owned(), None)),
            }
        }
    }

    #[SimpleObject]
    struct QueryContact {
        first_name: String,
        last_name: String,
    }

    impl std::convert::From<Contact> for QueryContact {
        fn from(c: Contact) -> Self {
            Self {
                first_name: c.first_name.to_owned(),
                last_name: c.last_name.to_owned(),
            }
        }
    }

    struct MutationRoot;

    #[Object]
    impl MutationRoot {
        async fn create(
            &self,
            ctx: &Context<'_>,
            #[arg(desc = "contact")] contact: MutationCreate,
        ) -> FieldResult<QueryContact> {
            let repo = ctx.data_unchecked::<FileRepository>();
            let model: Contact = contact.into();
            create(model, repo).map_or_else(
                |e| Err(FieldError(format!("{}", e).to_owned(), None)),
                |c| Ok(QueryContact::from(c)),
            )
        }
    }

    #[InputObject]
    struct MutationCreate {
        id: String,
        first_name: String,
        last_name: String,
    }

    impl std::convert::From<Contact> for MutationCreate {
        fn from(c: Contact) -> Self {
            Self {
                id: c.id.to_owned(),
                first_name: c.first_name.to_owned(),
                last_name: c.last_name.to_owned(),
            }
        }
    }

    impl std::convert::Into<Contact> for MutationCreate {
        fn into(self) -> Contact {
            Contact {
                id: self.id.to_owned(),
                first_name: self.first_name.to_owned(),
                last_name: self.last_name.to_owned(),
            }
        }
    }

    pub async fn start_server() -> std::io::Result<()> {
        let repo = FileRepository::new("/tmp");
        let local = tokio::task::LocalSet::new();
        let sys = actix_rt::System::run_in_tokio("server", &local);

        let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
            .data(repo)
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
