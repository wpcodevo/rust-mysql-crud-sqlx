use crate::{
    model::{NoteModel, NoteModelResponse},
    schema::{CreateNoteSchema, FilterOptions, UpdateNoteSchema},
    AppState,
};
use actix_web::{delete, get, patch, post, web, HttpResponse, Responder};
use serde_json::json;

#[get("/healthchecker")]
async fn health_checker_handler() -> impl Responder {
    const MESSAGE: &str = "Build Simple CRUD API with Rust, SQLX, MySQL, and Actix Web";

    HttpResponse::Ok().json(json!({"status": "success","message": MESSAGE}))
}

#[get("/notes")]
pub async fn note_list_handler(
    opts: web::Query<FilterOptions>,
    data: web::Data<AppState>,
) -> impl Responder {
    let limit = opts.limit.unwrap_or(10);
    let offset = (opts.page.unwrap_or(1) - 1) * limit;

    let notes: Vec<NoteModel> = sqlx::query_as!(
        NoteModel,
        r#"SELECT * FROM notes ORDER by id LIMIT ? OFFSET ?"#,
        limit as i32,
        offset as i32
    )
    .fetch_all(&data.db)
    .await
    .unwrap();

    let note_responses = notes
        .into_iter()
        .map(|note| filter_db_record(&note))
        .collect::<Vec<NoteModelResponse>>();

    let json_response = serde_json::json!({
        "status": "success",
        "results": note_responses.len(),
        "notes": note_responses
    });
    HttpResponse::Ok().json(json_response)
}

#[post("/notes/")]
async fn create_note_handler(
    body: web::Json<CreateNoteSchema>,
    data: web::Data<AppState>,
) -> impl Responder {
    let user_id = uuid::Uuid::new_v4().to_string();
    let query_result =
        sqlx::query(r#"INSERT INTO notes (id,title,content,category) VALUES (?, ?, ?, ?)"#)
            .bind(user_id.clone())
            .bind(body.title.to_string())
            .bind(body.content.to_string())
            .bind(body.category.to_owned().unwrap_or_default())
            .execute(&data.db)
            .await
            .map_err(|err: sqlx::Error| err.to_string());

    if let Err(err) = query_result {
        if err.contains("Duplicate entry") {
            return HttpResponse::BadRequest().json(
            serde_json::json!({"status": "fail","message": "Note with that title already exists"}),
        );
        }

        return HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error","message": format!("{:?}", err)}));
    }

    let query_result = sqlx::query_as!(NoteModel, r#"SELECT * FROM notes WHERE id = ?"#, user_id)
        .fetch_one(&data.db)
        .await;

    match query_result {
        Ok(note) => {
            let note_response = serde_json::json!({"status": "success","data": serde_json::json!({
                "note": filter_db_record(&note)
            })});

            return HttpResponse::Ok().json(note_response);
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error","message": format!("{:?}", e)}));
        }
    }
}

#[get("/notes/{id}")]
async fn get_note_handler(
    path: web::Path<uuid::Uuid>,
    data: web::Data<AppState>,
) -> impl Responder {
    let note_id = path.into_inner().to_string();
    let query_result = sqlx::query_as!(NoteModel, r#"SELECT * FROM notes WHERE id = ?"#, note_id)
        .fetch_one(&data.db)
        .await;

    match query_result {
        Ok(note) => {
            let note_response = serde_json::json!({"status": "success","data": serde_json::json!({
                "note": filter_db_record(&note)
            })});

            return HttpResponse::Ok().json(note_response);
        }
        Err(sqlx::Error::RowNotFound) => {
            return HttpResponse::NotFound().json(
            serde_json::json!({"status": "fail","message": format!("Note with ID: {} not found", note_id)}),
        );
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error","message": format!("{:?}", e)}));
        }
    };
}

#[patch("/notes/{id}")]
async fn edit_note_handler(
    path: web::Path<uuid::Uuid>,
    body: web::Json<UpdateNoteSchema>,
    data: web::Data<AppState>,
) -> impl Responder {
    let note_id = path.into_inner().to_string();
    let query_result = sqlx::query_as!(NoteModel, r#"SELECT * FROM notes WHERE id = ?"#, note_id)
        .fetch_one(&data.db)
        .await;

    let note = match query_result {
        Ok(note) => note,
        Err(sqlx::Error::RowNotFound) => {
            return HttpResponse::NotFound().json(
                serde_json::json!({"status": "fail","message": format!("Note with ID: {} not found", note_id)}),
            );
        }
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"status": "error","message": format!("{:?}", e)}));
        }
    };

    let published = body.published.unwrap_or(note.published != 0);
    let i8_publised = published as i8;

    let update_result = sqlx::query(
        r#"UPDATE notes SET title = ?, content = ?, category = ?, published = ? WHERE id = ?"#,
    )
    .bind(body.title.to_owned().unwrap_or_else(|| note.title.clone()))
    .bind(
        body.content
            .to_owned()
            .unwrap_or_else(|| note.content.clone()),
    )
    .bind(
        body.category
            .to_owned()
            .unwrap_or_else(|| note.category.clone().unwrap()),
    )
    .bind(i8_publised)
    .bind(note_id.to_owned())
    .execute(&data.db)
    .await;

    match update_result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                let message = format!("Note with ID: {} not found", note_id);
                return HttpResponse::NotFound().json(json!({"status": "fail","message": message}));
            }
        }
        Err(e) => {
            let message = format!("Internal server error: {}", e);
            return HttpResponse::InternalServerError()
                .json(json!({"status": "error","message": message}));
        }
    }

    let updated_note_result = sqlx::query_as!(
        NoteModel,
        r#"SELECT * FROM notes WHERE id = ?"#,
        note_id.to_owned()
    )
    .fetch_one(&data.db)
    .await;

    match updated_note_result {
        Ok(note) => {
            let note_response = serde_json::json!({"status": "success","data": serde_json::json!({
                "note": filter_db_record(&note)
            })});

            HttpResponse::Ok().json(note_response)
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"status": "error","message": format!("{:?}", e)})),
    }
}

#[delete("/notes/{id}")]
async fn delete_note_handler(
    path: web::Path<uuid::Uuid>,
    data: web::Data<AppState>,
) -> impl Responder {
    let note_id = path.into_inner().to_string();
    let query_result = sqlx::query!(r#"DELETE FROM notes WHERE id = ?"#, note_id)
        .execute(&data.db)
        .await;

    match query_result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                let message = format!("Note with ID: {} not found", note_id);
                HttpResponse::NotFound().json(json!({"status": "fail","message": message}))
            } else {
                HttpResponse::NoContent().finish()
            }
        }
        Err(e) => {
            let message = format!("Internal server error: {}", e);
            HttpResponse::InternalServerError().json(json!({"status": "error","message": message}))
        }
    }
}

fn filter_db_record(note: &NoteModel) -> NoteModelResponse {
    NoteModelResponse {
        id: note.id.to_owned(),
        title: note.title.to_owned(),
        content: note.content.to_owned(),
        category: note.category.to_owned().unwrap(),
        published: note.published != 0,
        createdAt: note.created_at.unwrap(),
        updatedAt: note.updated_at.unwrap(),
    }
}

pub fn config(conf: &mut web::ServiceConfig) {
    let scope = web::scope("/api")
        .service(health_checker_handler)
        .service(note_list_handler)
        .service(create_note_handler)
        .service(get_note_handler)
        .service(edit_note_handler)
        .service(delete_note_handler);

    conf.service(scope);
}
