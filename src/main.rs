#[macro_use]
extern crate log;

mod models {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Clone, Hash, Copy)]
    pub struct Contact<'a> {
        pub id: &'a str,
        pub first_name: &'a str,
        pub last_name: &'a str,
    }
}

mod usecases {

    use super::models::*;
    use super::repo::*;
    use std::error::Error;

    pub struct Contacts {}

    impl Contacts {
        pub fn create<'a>(
            contact: Contact<'a>,
            repo: &dyn Repository<Contact<'a>>,
        ) -> Result<Contact<'a>, Box<dyn Error>> {
            let r = repo.set(contact.clone())?;
            println!("contact created {:?}", contact);
            Ok(r)
        }

        pub fn get<'a>(
            id: &str,
            repo: &dyn Repository<Contact<'a>>,
        ) -> Result<Contact<'a>, Box<dyn Error>> {
            repo.get(id)
        }
    }
}

mod repo {
    use serde::de::Deserialize;
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

    impl<'a, T: 'a + Copy + Deserialize<'a> + Serialize + Hash> Repository<T> for FileRepository<'a> {
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
            use std::io::prelude::*;
            use std::path::Path;
            let path = Path::new(&self.path).join(format!("{}.json", id));
            println!("{:?}", path);
            let mut f = File::open(&path)?;
            let mut buf = String::new();
            f.read_to_string(&mut buf)?;
            let result: T = serde_json::from_str(&buf.clone()).expect("Unable to serialized");
            Ok(result.clone())
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

    #[Object]
    impl Contact<'_> {
        async fn id(&self) -> String {
            self.id.to_string()
        }

        async fn first_name(&self) -> String {
            self.first_name.to_string()
        }

        async fn last_name(&self) -> String {
            self.last_name.to_string()
        }
    }

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
        async fn get<'a>(&self, ctx: &Context<'a>, id: String) -> FieldResult<Contact<'_>> {
            let repo = ctx.data_unchecked::<FileRepository<'static>>();
            match Contacts::get(id.as_str(), repo) {
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

    impl<'a> std::convert::From<Contact<'a>> for QueryContact {
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
            contact: MutationCreate,
        ) -> FieldResult<QueryContact> {
            let repo = ctx.data_unchecked::<FileRepository>();
            let model: Contact = contact.into();
            Contacts::create(model, repo).map_or_else(
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

    impl std::convert::From<Contact<'_>> for MutationCreate {
        fn from(c: Contact) -> Self {
            Self {
                id: c.id.to_owned(),
                first_name: c.first_name.to_owned(),
                last_name: c.last_name.to_owned(),
            }
        }
    }

    impl<'a> std::convert::Into<Contact<'a>> for MutationCreate {
        fn into(self) -> Contact<'a> {
            Contact {
                id: self.id.as_str(),
                first_name: self.first_name.as_str(),
                last_name: self.last_name.as_str(),
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
